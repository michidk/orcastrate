use std::collections::HashMap;
use std::path::Path;
use tera::Tera;

use crate::error::Error;

pub struct TemplateRenderer {
    tera: Tera,
}

impl TemplateRenderer {
    pub fn new(templates_dir: &Path) -> crate::error::Result<Self> {
        let glob = format!("{}/**/*.yml", templates_dir.display());
        let mut tera = Tera::new();
        tera.load_from_glob(&glob).map_err(|e| {
            Error::Template(format!(
                "failed to load templates from {}: {e}",
                templates_dir.display()
            ))
        })?;

        Ok(Self { tera })
    }

    pub fn render(
        &self,
        template_id: &str,
        params: &HashMap<String, serde_norway::Value>,
    ) -> crate::error::Result<String> {
        let template_name = format!("{template_id}.yml");

        let mut context = tera::Context::new();
        for (key, value) in params {
            let json_value: serde_json::Value =
                serde_json::to_value(value).map_err(|e| Error::Render {
                    template: template_id.to_string(),
                    message: format!("failed to convert param '{key}': {e}"),
                })?;
            context.insert(key.clone(), &json_value);
        }

        let rendered = self
            .tera
            .render(&template_name, &context)
            .map_err(|e| Error::Render {
                template: template_id.to_string(),
                message: format!("{e}"),
            })?;

        validate_yaml(&rendered, template_id)?;

        Ok(rendered)
    }

    pub fn list_templates(&self) -> Vec<String> {
        self.tera
            .get_template_names()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn render_workflow(&self, content: &str) -> anyhow::Result<String> {
        let parsed = super::frontmatter::parse(content)?;
        let fm = parsed
            .frontmatter
            .ok_or_else(|| anyhow::anyhow!("workflow has no Orcastrate frontmatter"))?;
        let rendered = self.render(&fm.template, &fm.params)?;
        Ok(format!("{}\n\n{rendered}", parsed.raw_block.unwrap()))
    }
}

fn validate_yaml(content: &str, template_id: &str) -> crate::error::Result<()> {
    let _: serde_norway::Value = serde_norway::from_str(content).map_err(|e| {
        Error::YamlValidation(format!(
            "rendered template '{template_id}' produced invalid YAML: {e}"
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_rust_feature_matrix() {
        let templates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let renderer = TemplateRenderer::new(&templates_dir).unwrap();
        let params: HashMap<String, serde_norway::Value> =
            serde_norway::from_str("features:\n  - serde\n  - async\n").unwrap();

        let rendered = renderer.render("rust-ci", &params).unwrap();

        assert!(rendered.contains("feature: [\"serde\", \"async\"]"));
    }

    #[test]
    fn renders_managed_workflow_and_rejects_missing_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.yml"), "name: {{ name }}\n").unwrap();
        let renderer = TemplateRenderer::new(dir.path()).unwrap();
        let header =
            "# @orcastrate\n# template: test\n# params:\n#   name: Example\n# @end-orcastrate";
        assert_eq!(
            renderer
                .render_workflow(&format!("{header}\n\nold content"))
                .unwrap(),
            format!("{header}\n\nname: Example\n")
        );
        assert!(renderer.render_workflow("name: Unmanaged\n").is_err());
    }
}
