use core::fmt;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UxNodeId(String);

impl UxNodeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UxNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for UxNodeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for UxNodeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}
