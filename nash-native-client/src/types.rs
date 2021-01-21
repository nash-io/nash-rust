
#[derive(Clone, Copy)]
pub enum Environment {
    Production,
    Sandbox,
    Dev(&'static str),
}

impl Environment {
    pub fn url(&self) -> &str {
        match self {
            Self::Production => "app.nash.io",
            Self::Sandbox => "app.sandbox.nash.io",
            Self::Dev(s) => s,
        }
    }
}
