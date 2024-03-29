
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

impl From<openlimits_exchange::exchange::Environment> for Environment {
    fn from(from: openlimits_exchange::exchange::Environment) -> Self {
        match from {
            openlimits_exchange::exchange::Environment::Production => Environment::Production,
            openlimits_exchange::exchange::Environment::Sandbox => Environment::Sandbox
        }
    }
}