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
        let tera = Tera::new(&glob).map_err(|e| {
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
            context.insert(key, &json_value);
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
}

fn validate_yaml(content: &str, template_id: &str) -> crate::error::Result<()> {
    let _: serde_norway::Value = serde_norway::from_str(content).map_err(|e| {
        Error::YamlValidation(format!(
            "rendered template '{template_id}' produced invalid YAML: {e}"
        ))
    })?;
    Ok(())
}
