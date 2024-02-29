use std::fmt;

/// The result of a failed TryFrom<String> conversion for [`Extension`]
///
/// Contains the string that was being converted
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ExtensionParseError(pub String);

impl fmt::Display for ExtensionParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "unknown attribute {:?}, extensions must start with 'x-' (see https://docs.docker.com/compose/compose-file/#extension)", self.0)
    }
}

impl std::error::Error for ExtensionParseError {}
