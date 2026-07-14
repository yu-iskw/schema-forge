#[must_use]
pub fn render_greeting(project_name: &str) -> String {
    format!("Welcome to {project_name}")
}

#[cfg(test)]
mod tests {
    use super::render_greeting;

    #[test]
    fn renders_project_name() {
        assert_eq!(render_greeting("workspace-cli"), "Welcome to workspace-cli");
    }
}
