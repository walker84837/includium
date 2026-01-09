use crate::token::Token;

/// A preprocessor macro definition
#[derive(Clone, Debug)]
pub struct Macro {
    pub(crate) params: Option<Vec<String>>,
    pub(crate) body: Vec<Token>,
    pub(crate) is_variadic: bool,
}
