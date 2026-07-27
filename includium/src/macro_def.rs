use std::rc::Rc;

use crate::token::Token;

/// A preprocessor macro definition.
///
/// This struct is opaque to external consumers -- all fields are `pub(crate)`.
/// Obtain instances via [`PreprocessorDriver::get_macros()`](crate::PreprocessorDriver::get_macros).
#[derive(Clone, Debug)]
pub struct Macro {
    pub(crate) params: Option<Vec<String>>,
    pub(crate) body: Rc<Vec<Token>>,
    pub(crate) is_variadic: bool,
    #[allow(dead_code)] // For future tooling integration
    pub(crate) definition_location: Option<(String, usize)>,
    #[allow(dead_code)] // For future tooling integration
    pub(crate) is_builtin: bool,
}
