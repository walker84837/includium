use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;

use crate::config::{
    Compiler, IncludeContext, IncludeKind, IncludeResolver, PreprocessorConfig, Target,
    WarningHandler,
};
use crate::date_time::{format_date, format_time};
use crate::error::PreprocessError;
use crate::macro_def::Macro;
use crate::token::{ExprToken, Token, is_identifier_continue, is_identifier_start};

type MacroArguments = Vec<Vec<Token>>;
type MacroParseResult = (MacroArguments, usize);

#[derive(Clone, Debug)]
enum ConditionalState {
    If(bool),
    Elif(bool),
    Else(bool),
}

/// The main C preprocessor struct
pub struct Preprocessor {
    macros: HashMap<String, Macro>,
    pub(crate) include_resolver: Option<IncludeResolver>,
    pub(crate) recursion_limit: usize,
    conditional_stack: Vec<ConditionalState>,
    pub(crate) current_line: usize,
    pub(crate) current_file: String,
    included_once: HashSet<String>,
    include_stack: Vec<String>,
    disabled_macros: HashSet<String>,
    warning_handler: Option<WarningHandler>,
    compiler: Compiler,
}

impl Default for Preprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Preprocessor {
    /// Create a new preprocessor instance
    #[must_use]
    pub fn new() -> Self {
        Preprocessor {
            macros: HashMap::new(),
            include_resolver: None,
            recursion_limit: 128,
            conditional_stack: Vec::new(),
            current_line: 1,
            current_file: "<stdin>".to_string(),
            included_once: HashSet::new(),
            include_stack: Vec::new(),
            disabled_macros: HashSet::new(),
            warning_handler: None,
            compiler: Compiler::GCC,
        }
    }

    /// Create a preprocessor with the given configuration
    #[must_use]
    pub fn with_config(config: &PreprocessorConfig) -> Self {
        let mut pp = Self::new();
        pp.apply_config(config);
        pp
    }

    /// Apply configuration to the preprocessor
    pub fn apply_config(&mut self, config: &PreprocessorConfig) {
        self.compiler = config.compiler.clone();
        self.recursion_limit = config.recursion_limit;
        self.include_resolver.clone_from(&config.include_resolver);
        self.warning_handler.clone_from(&config.warning_handler);

        self.define_target_macros(&config.target);
        self.define_compiler_macros(&config.compiler);

        self.stub_compiler_intrinsics();
        self.define_sizeof_stubs();
    }

    fn define_target_macros(&mut self, target: &Target) {
        match target {
            Target::Linux => {
                self.define_builtin("__linux__", None, "1", false);
                self.define_builtin("__unix__", None, "1", false);
                self.define_builtin("__LP64__", None, "1", false);
            }
            Target::Windows => {
                self.define_builtin("_WIN32", None, "1", false);
                self.define_builtin("WIN32", None, "1", false);
                self.define_builtin("_WINDOWS", None, "1", false);
            }
            Target::MacOS => {
                self.define_builtin("__APPLE__", None, "1", false);
                self.define_builtin("__MACH__", None, "1", false);
                self.define_builtin("TARGET_OS_MAC", None, "1", false);
                self.define_builtin("__LP64__", None, "1", false);
            }
        }
    }

    fn define_compiler_macros(&mut self, compiler: &Compiler) {
        match compiler {
            Compiler::GCC => {
                // GCC 11.2.0 (common in many Linux distributions)
                self.define_builtin("__GNUC__", None, "11", false);
                self.define_builtin("__GNUC_MINOR__", None, "2", false);
                self.define_builtin("__GNUC_PATCHLEVEL__", None, "0", false);
                self.define_builtin("_GNU_SOURCE", None, "1", false);
            }
            Compiler::Clang => {
                // Clang 14.0.0 (matches Xcode 13.1)
                self.define_builtin("__clang__", None, "1", false);
                self.define_builtin("__clang_major__", None, "14", false);
                self.define_builtin("__clang_minor__", None, "0", false);
                self.define_builtin("__clang_patchlevel__", None, "0", false);
            }
            Compiler::MSVC => {
                // MSVC 19.20 (Visual Studio 2019)
                self.define_builtin("_MSC_VER", None, "1920", false);
                self.define_builtin("_MSC_FULL_VER", None, "192027508", false);
                self.define_builtin("WIN32_LEAN_AND_MEAN", None, "", false);
                self.define_builtin("_CRT_SECURE_NO_WARNINGS", None, "", false);
            }
        }
    }

    /// Add a custom include resolver function
    #[must_use]
    pub fn with_include_resolver<F>(mut self, f: F) -> Self
    where
        F: Fn(&str, crate::config::IncludeKind, &crate::config::IncludeContext) -> Option<String>
            + 'static,
    {
        self.include_resolver = Some(Rc::new(f));
        self
    }

    /// Set the maximum recursion depth for macro expansion
    pub fn set_recursion_limit(&mut self, limit: usize) {
        self.recursion_limit = limit;
    }

    /// Set the current file name for error reporting
    pub fn set_current_file(&mut self, file: String) {
        self.current_file = file;
    }

    /// Define a preprocessor macro
    pub fn define<S: AsRef<str>>(
        &mut self,
        name: S,
        params: Option<Vec<String>>,
        body: S,
        is_variadic: bool,
    ) {
        self.define_macro(name, params, body, is_variadic, false);
    }

    fn define_builtin<S: AsRef<str>>(
        &mut self,
        name: S,
        params: Option<Vec<String>>,
        body: S,
        is_variadic: bool,
    ) {
        self.define_macro(name, params, body, is_variadic, true);
    }

    fn define_macro<S: AsRef<str>>(
        &mut self,
        name: S,
        params: Option<Vec<String>>,
        body: S,
        is_variadic: bool,
        is_builtin: bool,
    ) {
        let stripped_body = Self::strip_comments(body.as_ref());
        let body_tokens = Self::tokenize_line(stripped_body.as_str());
        self.macros.insert(
            name.as_ref().to_string(),
            Macro {
                params,
                body: Rc::new(body_tokens),
                is_variadic,
                definition_location: if is_builtin {
                    None
                } else {
                    Some((self.current_file.clone(), self.current_line))
                },
                is_builtin,
            },
        );
    }

    /// Remove a macro definition
    pub fn undef(&mut self, name: &str) {
        self.macros.remove(name);
    }

    /// Get a reference to the defined macros
    #[must_use]
    pub fn get_macros(&self) -> &HashMap<String, Macro> {
        &self.macros
    }

    /// Check if a macro is defined
    #[must_use]
    pub fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    /// Process the input C code and return the preprocessed result
    ///
    /// # Errors
    /// Returns `PreprocessError` if there's a malformed directive,
    /// macro recursion limit is exceeded, or conditional blocks are unterminated.
    pub fn process(&mut self, input: &str) -> Result<String, PreprocessError> {
        let spliced = Self::line_splice(input);
        let pragma_processed = Self::process_pragma(&spliced);
        let mut out_lines: Vec<String> = Vec::new();
        self.conditional_stack.clear();
        self.current_line = 1;

        for line in pragma_processed.lines() {
            if let Some(directive) = Self::extract_directive(line) {
                if let Some(content) = self.handle_directive(directive, line)? {
                    out_lines.push(content);
                }
            } else if self.can_emit_line() {
                let tokens = Self::tokenize_line(line);
                let expanded_tokens = self.expand_tokens(&tokens, 0)?;
                let reconstructed = Self::tokens_to_string(&expanded_tokens);
                out_lines.push(reconstructed);
            }
            self.current_line += 1;
        }

        if !self.conditional_stack.is_empty() {
            return Err(PreprocessError::ConditionalError(
                "unterminated #if/#ifdef/#ifndef".to_string(),
            ));
        }

        Ok(out_lines.join("\n"))
    }

    /// Checks if the current line should be emitted in the output based on the active
    /// state of conditional compilation directives (#if, #ifdef, #else, etc.).
    ///
    /// Returns `true` if all conditions in the conditional stack are active (true),
    /// meaning the line should be included. Returns `false` if any condition is inactive,
    /// indicating the line should be skipped.
    fn can_emit_line(&self) -> bool {
        for state in &self.conditional_stack {
            let active = match state {
                ConditionalState::If(a) | ConditionalState::Elif(a) | ConditionalState::Else(a) => {
                    *a
                }
            };
            if !active {
                return false;
            }
        }
        true
    }

    fn line_splice(input: &str) -> String {
        if !input.contains('\\') {
            return input.to_string();
        }

        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(&next) = chars.peek() {
                    if next == '\n' {
                        chars.next();
                    } else if next == '\r' {
                        chars.next();
                        if let Some(&next2) = chars.peek()
                            && next2 == '\n'
                        {
                            chars.next();
                        }
                    } else {
                        out.push(ch);
                    }
                } else {
                    out.push(ch);
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    /// Process _Pragma operators in a line, replacing with #pragma directives
    fn process_pragma(line: &str) -> String {
        let mut result = String::with_capacity(line.len());
        let mut i = 0;
        let chars: Vec<char> = line.chars().collect();

        while i < chars.len() {
            if i + 7 <= chars.len() && chars[i..i + 7] == ['_', 'P', 'r', 'a', 'g', 'm', 'a'] {
                // Found _Pragma, look for (
                let mut j = i + 7;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && chars[j] == '(' {
                    j += 1;
                    // Parse the string
                    if j < chars.len() && chars[j] == '"' {
                        j += 1;
                        let mut string_content = String::new();
                        while j < chars.len() {
                            if chars[j] == '"' {
                                // Check for escape
                                let mut backslash_count = 0;
                                let mut k = j - 1;
                                while k > 0 && chars[k] == '\\' {
                                    backslash_count += 1;
                                    k -= 1;
                                }
                                if backslash_count % 2 == 0 {
                                    // End of string
                                    break;
                                } else {
                                    string_content.push(chars[j]);
                                }
                            } else {
                                string_content.push(chars[j]);
                            }
                            j += 1;
                        }
                        if j < chars.len() && chars[j] == '"' {
                            j += 1;
                            // Skip whitespace and )
                            while j < chars.len() && chars[j].is_whitespace() {
                                j += 1;
                            }
                            if j < chars.len() && chars[j] == ')' {
                                j += 1;
                                // Replace with #pragma
                                result.push_str("#pragma ");
                                result.push_str(&string_content);
                                i = j;
                                continue;
                            }
                        }
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
        }
        result
    }

    fn extract_directive(line: &str) -> Option<&str> {
        let trimmed = line.trim_start();
        trimmed.strip_prefix('#').map(str::trim)
    }

    fn handle_directive(
        &mut self,
        directive: &str,
        full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
        let mut parts = directive.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").trim();
        let rest = parts.next().unwrap_or("").trim();

        match cmd {
            "define" => self.handle_define(rest),
            "undef" => self.handle_undef(rest),
            "include" => self.handle_include(rest),
            "ifdef" => {
                self.handle_ifdef(rest);
                Ok(None)
            }
            "ifndef" => {
                self.handle_ifndef(rest);
                Ok(None)
            }
            "if" => self.handle_if(rest),
            "elif" => self.handle_elif(rest, full_line),
            "else" => self.handle_else(),
            "endif" => self.handle_endif(),
            "error" => self.handle_error(rest),
            "warning" => {
                self.handle_warning(rest);
                Ok(None)
            }
            "line" => self.handle_line(rest),
            "pragma" => {
                self.handle_pragma(rest);
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn handle_define(&mut self, rest: &str) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let rest = rest.trim_start();
        if rest.is_empty() {
            return Err(PreprocessError::MalformedDirective("define".to_string()));
        }

        let mut chars = rest.chars().peekable();
        let mut name = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                name.push(c);
                chars.next();
            } else {
                break;
            }
        }

        if name.is_empty() {
            return Err(PreprocessError::MalformedDirective("define".to_string()));
        }

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        let mut params: Option<Vec<String>> = None;
        let mut is_variadic = false;

        if let Some(&'(') = chars.peek() {
            chars.next();
            let mut param = String::new();
            let mut params_vec = Vec::new();

            loop {
                match chars.peek() {
                    None => return Err(PreprocessError::MalformedDirective("define".to_string())),
                    Some(&')') => {
                        if !param.trim().is_empty() {
                            params_vec.push(param.trim().to_string());
                        }
                        chars.next();
                        break;
                    }
                    Some(&',') => {
                        params_vec.push(param.trim().to_string());
                        param.clear();
                        chars.next();
                    }
                    Some(&'.') => {
                        is_variadic = true;
                        chars.next();
                        if chars.peek() == Some(&'.') {
                            chars.next();
                            if chars.peek() == Some(&'.') {
                                chars.next();
                                break;
                            }
                        }
                    }
                    Some(&c) => {
                        param.push(c);
                        chars.next();
                    }
                }
            }
            params = Some(params_vec);
        }

        let body_str: String = chars.collect();
        let stripped = Self::strip_comments(&body_str);
        let stripped_body = stripped.trim();
        let body_tokens = Self::tokenize_line(stripped_body);
        self.macros.insert(
            name,
            Macro {
                params,
                body: Rc::new(body_tokens),
                is_variadic,
                definition_location: Some((self.current_file.clone(), self.current_line)),
                is_builtin: false,
            },
        );
        Ok(None)
    }

    fn handle_undef(&mut self, rest: &str) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let name = rest.split_whitespace().next().unwrap_or("");
        if name.is_empty() {
            Err(PreprocessError::MalformedDirective("undef".to_string()))
        } else {
            self.macros.remove(name);
            Ok(None)
        }
    }

    fn handle_include(&mut self, rest: &str) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let trimmed = rest.trim();
        let (path, kind) =
            if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
                (
                    Some(trimmed[1..(trimmed.len() - 1)].to_string()),
                    IncludeKind::Local,
                )
            } else if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() >= 2 {
                (
                    Some(trimmed[1..(trimmed.len() - 1)].to_string()),
                    IncludeKind::System,
                )
            } else {
                (None, IncludeKind::Local) // dummy
            };

        let Some(p) = path else {
            return Err(PreprocessError::MalformedDirective("include".to_string()));
        };

        let Some(resolver) = &self.include_resolver else {
            return Err(PreprocessError::IncludeNotFound(p));
        };

        let context = IncludeContext {
            include_stack: self.include_stack.clone(),
            include_dirs: Vec::new(), // TODO: populate from config or env
        };

        let Some(content) = resolver(&p, kind, &context) else {
            return Err(PreprocessError::IncludeNotFound(p));
        };

        // Check for cycles
        if self.include_stack.contains(&p) {
            return Err(PreprocessError::Other(format!(
                "Include cycle detected for '{}'",
                p
            )));
        }

        // Check for #pragma once
        if content.contains("#pragma once") && self.included_once.contains(&p) {
            return Ok(Some(String::new())); // Skip inclusion
        }

        self.include_stack.push(self.current_file.clone());
        let mut nested = Preprocessor {
            macros: self.macros.clone(),
            include_resolver: self.include_resolver.clone(),
            recursion_limit: self.recursion_limit,
            conditional_stack: Vec::new(),
            current_line: 1,
            current_file: p.clone(),
            included_once: self.included_once.clone(),
            include_stack: self.include_stack.clone(),
            disabled_macros: HashSet::new(),
            warning_handler: self.warning_handler.clone(),
            compiler: self.compiler.clone(),
        };
        let processed = nested.process(&content)?;
        self.include_stack.pop();
        self.macros = nested.macros;

        // Mark as included if it has #pragma once
        if content.contains("#pragma once") {
            self.included_once.insert(p);
        }

        Ok(Some(processed))
    }

    fn handle_ifdef(&mut self, rest: &str) {
        let name = rest.trim();
        let defined = self.is_defined(name);
        self.conditional_stack.push(ConditionalState::If(defined));
    }

    fn handle_ifndef(&mut self, rest: &str) {
        let name = rest.trim();
        let defined = self.is_defined(name);
        self.conditional_stack.push(ConditionalState::If(!defined));
    }

    fn handle_if(&mut self, rest: &str) -> Result<Option<String>, PreprocessError> {
        let evaluated = self.evaluate_expression(rest)?;
        self.conditional_stack.push(ConditionalState::If(evaluated));
        Ok(None)
    }

    fn handle_elif(
        &mut self,
        rest: &str,
        _full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
        if self.conditional_stack.is_empty() {
            return Err(PreprocessError::ConditionalError(
                "#elif without #if".to_string(),
            ));
        }

        let evaluated = self.evaluate_expression(rest)?;

        if let Some(last) = self.conditional_stack.last_mut() {
            *last = ConditionalState::Elif(evaluated);
        }
        Ok(None)
    }

    fn handle_else(&mut self) -> Result<Option<String>, PreprocessError> {
        let is_active = if let Some(last) = self.conditional_stack.last() {
            matches!(
                last,
                ConditionalState::If(false) | ConditionalState::Elif(false)
            )
        } else {
            return Err(PreprocessError::ConditionalError(
                "#else without #if".to_string(),
            ));
        };

        if let Some(last) = self.conditional_stack.last_mut() {
            *last = ConditionalState::Else(is_active);
        }
        Ok(None)
    }

    fn handle_endif(&mut self) -> Result<Option<String>, PreprocessError> {
        if self.conditional_stack.pop().is_none() {
            return Err(PreprocessError::ConditionalError(
                "#endif without #if".to_string(),
            ));
        }
        Ok(None)
    }

    fn handle_error(&mut self, rest: &str) -> Result<Option<String>, PreprocessError> {
        if self.can_emit_line() {
            let msg = if rest.is_empty() {
                "#error directive".to_string()
            } else {
                format!("#error: {rest}")
            };
            Err(PreprocessError::Other(msg))
        } else {
            Ok(None)
        }
    }

    fn handle_warning(&mut self, rest: &str) {
        if self.can_emit_line() && matches!(self.compiler, Compiler::GCC | Compiler::Clang) {
            let msg = if rest.is_empty() {
                "#warning directive".to_string()
            } else {
                format!("#warning: {rest}")
            };
            if let Some(ref handler) = self.warning_handler {
                handler(&msg);
            }
        }
    }

    fn handle_line(&mut self, rest: &str) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.is_empty() {
            return Err(PreprocessError::MalformedDirective("line".to_string()));
        }

        if let Ok(line_num) = parts[0].parse::<usize>() {
            self.current_line = line_num.saturating_sub(1);
            if parts.len() > 1 {
                let filename = parts[1];
                // Remove surrounding quotes if present
                let filename = if let Some(stripped) = filename.strip_prefix('"') {
                    stripped.strip_suffix('"').unwrap_or(stripped)
                } else {
                    filename
                };
                self.current_file = filename.to_string();
            }
        }
        Ok(None)
    }

    fn handle_pragma(&mut self, rest: &str) {
        let trimmed = rest.trim();
        if trimmed == "once" {
            self.included_once.insert(self.current_file.clone());
        }
    }

    fn evaluate_expression(&mut self, expr: &str) -> Result<bool, PreprocessError> {
        let tokens = Self::tokenize_line(expr);
        let expanded = self.expand_tokens(&tokens, 0)?;
        let expr_str = Self::tokens_to_string(&expanded);
        let trimmed = expr_str.trim();

        // Handle defined() specially
        if trimmed == "defined" || trimmed.starts_with("defined") {
            let identifier =
                if let (Some(start), Some(end)) = (trimmed.find('('), trimmed.find(')')) {
                    trimmed[start + 1..end].trim()
                } else {
                    trimmed.strip_prefix("defined").unwrap_or(trimmed).trim()
                };
            return Ok(self.is_defined(identifier));
        }

        // Parse the expression
        self.parse_expression(trimmed)
    }

    /// Parse a preprocessor expression with full operator support
    ///
    /// # Errors
    /// Returns `PreprocessError` if the expression is malformed or has invalid operators.
    pub fn parse_expression(&mut self, expr: &str) -> Result<bool, PreprocessError> {
        let tokens = Self::tokenize_expression(expr)?;
        let result = self.evaluate_expression_tokens(&tokens)?;
        Ok(result != 0)
    }

    /// Tokenize expression string into expression tokens
    fn tokenize_expression(expr: &str) -> Result<Vec<ExprToken>, PreprocessError> {
        let mut tokens = Vec::new();
        let mut chars = expr.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '0'..='9' => {
                    let mut num = String::new();
                    num.push(ch);
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() {
                            num.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if let Ok(val) = num.parse::<i64>() {
                        tokens.push(ExprToken::Number(val));
                    } else {
                        return Err(PreprocessError::Other(format!("Invalid number: {num}")));
                    }
                }
                'a'..='z' | 'A'..='Z' | '_' => {
                    let mut ident = String::new();
                    ident.push(ch);
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            ident.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    tokens.push(ExprToken::Identifier(ident));
                }
                '(' => tokens.push(ExprToken::LParen),
                ')' => tokens.push(ExprToken::RParen),
                '!' => {
                    if let Some(&'=') = chars.peek() {
                        chars.next();
                        tokens.push(ExprToken::NotEqual);
                    } else {
                        tokens.push(ExprToken::Not);
                    }
                }
                '=' => {
                    if let Some(&'=') = chars.peek() {
                        chars.next();
                        tokens.push(ExprToken::Equal);
                    } else {
                        return Err(PreprocessError::Other("Invalid operator: =".to_string()));
                    }
                }
                '<' => {
                    if let Some(&'=') = chars.peek() {
                        chars.next();
                        tokens.push(ExprToken::LessEqual);
                    } else {
                        tokens.push(ExprToken::Less);
                    }
                }
                '>' => {
                    if let Some(&'=') = chars.peek() {
                        chars.next();
                        tokens.push(ExprToken::GreaterEqual);
                    } else {
                        tokens.push(ExprToken::Greater);
                    }
                }
                '&' => {
                    if let Some(&'&') = chars.peek() {
                        chars.next();
                        tokens.push(ExprToken::And);
                    } else {
                        return Err(PreprocessError::Other("Invalid operator: &".to_string()));
                    }
                }
                '|' => {
                    if let Some(&'|') = chars.peek() {
                        chars.next();
                        tokens.push(ExprToken::Or);
                    } else {
                        return Err(PreprocessError::Other("Invalid operator: |".to_string()));
                    }
                }
                '+' => tokens.push(ExprToken::Plus),
                '-' => tokens.push(ExprToken::Minus),
                '*' => tokens.push(ExprToken::Multiply),
                '/' => tokens.push(ExprToken::Divide),
                '%' => tokens.push(ExprToken::Modulo),
                c if c.is_whitespace() => {}
                _ => return Err(PreprocessError::Other(format!("Invalid character: {ch}"))),
            }
        }

        Ok(tokens)
    }

    /// Evaluate expression tokens using recursive descent
    fn evaluate_expression_tokens(&self, tokens: &[ExprToken]) -> Result<i64, PreprocessError> {
        let mut pos = 0;
        let result = self.parse_or(tokens, &mut pos)?;
        if pos != tokens.len() {
            return Err(PreprocessError::Other(
                "Unexpected tokens at end of expression".to_string(),
            ));
        }
        Ok(result)
    }

    fn parse_or(&self, tokens: &[ExprToken], pos: &mut usize) -> Result<i64, PreprocessError> {
        let mut left = self.parse_and(tokens, pos)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Or => {
                    *pos += 1;
                    let right = self.parse_and(tokens, pos)?;
                    left = i64::from(left != 0 || right != 0);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_and(&self, tokens: &[ExprToken], pos: &mut usize) -> Result<i64, PreprocessError> {
        let mut left = self.parse_comparison(tokens, pos)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::And => {
                    *pos += 1;
                    let right = self.parse_comparison(tokens, pos)?;
                    left = i64::from(left != 0 && right != 0);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_comparison(
        &self,
        tokens: &[ExprToken],
        pos: &mut usize,
    ) -> Result<i64, PreprocessError> {
        let left = self.parse_additive(tokens, pos)?;
        if *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Equal => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos)?;
                    return Ok(i64::from(left == right));
                }
                ExprToken::NotEqual => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos)?;
                    return Ok(i64::from(left != right));
                }
                ExprToken::Less => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos)?;
                    return Ok(i64::from(left < right));
                }
                ExprToken::LessEqual => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos)?;
                    return Ok(i64::from(left <= right));
                }
                ExprToken::Greater => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos)?;
                    return Ok(i64::from(left > right));
                }
                ExprToken::GreaterEqual => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos)?;
                    return Ok(i64::from(left >= right));
                }
                _ => { /* not a comparison operator */ }
            }
        }
        Ok(left)
    }

    fn parse_additive(
        &self,
        tokens: &[ExprToken],
        pos: &mut usize,
    ) -> Result<i64, PreprocessError> {
        let mut left = self.parse_multiplicative(tokens, pos)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Plus => {
                    *pos += 1;
                    let right = self.parse_multiplicative(tokens, pos)?;
                    left += right;
                }
                ExprToken::Minus => {
                    *pos += 1;
                    let right = self.parse_multiplicative(tokens, pos)?;
                    left -= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(
        &self,
        tokens: &[ExprToken],
        pos: &mut usize,
    ) -> Result<i64, PreprocessError> {
        let mut left = self.parse_unary(tokens, pos)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Multiply => {
                    *pos += 1;
                    let right = self.parse_unary(tokens, pos)?;
                    left *= right;
                }
                ExprToken::Divide => {
                    *pos += 1;
                    let right = self.parse_unary(tokens, pos)?;
                    if right == 0 {
                        return Err(PreprocessError::Other("Division by zero".to_string()));
                    }
                    left /= right;
                }
                ExprToken::Modulo => {
                    *pos += 1;
                    let right = self.parse_unary(tokens, pos)?;
                    if right == 0 {
                        return Err(PreprocessError::Other("Modulo by zero".to_string()));
                    }
                    left %= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&self, tokens: &[ExprToken], pos: &mut usize) -> Result<i64, PreprocessError> {
        if *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Not => {
                    *pos += 1;
                    let expr = self.parse_unary(tokens, pos)?;
                    return Ok(i64::from(expr == 0));
                }
                ExprToken::Minus => {
                    *pos += 1;
                    let expr = self.parse_unary(tokens, pos)?;
                    return Ok(-expr);
                }
                _ => { /* not a unary operator */ }
            }
        }
        self.parse_primary(tokens, pos)
    }

    fn parse_primary(&self, tokens: &[ExprToken], pos: &mut usize) -> Result<i64, PreprocessError> {
        if *pos >= tokens.len() {
            return Err(PreprocessError::Other(
                "Unexpected end of expression".to_string(),
            ));
        }

        match &tokens[*pos] {
            ExprToken::Number(val) => {
                *pos += 1;
                Ok(*val)
            }
            ExprToken::Identifier(ident) => {
                *pos += 1;
                if ident == "defined" {
                    // Handle defined(identifier) or defined identifier
                    if *pos < tokens.len() && matches!(tokens[*pos], ExprToken::LParen) {
                        *pos += 1; // consume (
                        if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::Identifier(_))
                        {
                            return Err(PreprocessError::Other(
                                "Expected identifier after defined(".to_string(),
                            ));
                        }
                        if let ExprToken::Identifier(id) = &tokens[*pos] {
                            *pos += 1; // consume identifier
                            if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::RParen) {
                                return Err(PreprocessError::Other(
                                    "Expected ) after defined(identifier".to_string(),
                                ));
                            }
                            *pos += 1; // consume )
                            Ok(i64::from(self.is_defined(id)))
                        } else {
                            unreachable!()
                        }
                    } else {
                        Err(PreprocessError::Other(
                            "defined must be followed by identifier or (identifier)".to_string(),
                        ))
                    }
                } else {
                    // Unknown identifier, treat as 0 (common in preprocessors)
                    Ok(0)
                }
            }
            ExprToken::LParen => {
                *pos += 1;
                let expr = self.parse_or(tokens, pos)?;
                if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::RParen) {
                    return Err(PreprocessError::Other("Expected )".to_string()));
                }
                *pos += 1;
                Ok(expr)
            }
            _ => Err(PreprocessError::Other(
                "Expected number, identifier, or (".to_string(),
            )),
        }
    }

    fn tokenize_line(line: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut it = line.chars().peekable();

        while let Some(&ch) = it.peek() {
            if is_identifier_start(ch) {
                let mut s = String::new();
                while let Some(&c2) = it.peek() {
                    if is_identifier_continue(c2) {
                        s.push(c2);
                        it.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Identifier(s));
            } else if ch == '"' || ch == '\'' {
                let quote = ch;
                let mut s = String::new();
                s.push(quote);
                it.next();
                while let Some(c2) = it.next() {
                    s.push(c2);
                    if c2 == '\\' {
                        if let Some(c3) = it.next() {
                            s.push(c3);
                        }
                    } else if c2 == quote {
                        break;
                    }
                }
                if quote == '"' {
                    tokens.push(Token::StringLiteral(s));
                } else {
                    tokens.push(Token::CharLiteral(s));
                }
            } else if ch == '/' {
                it.next();
                if let Some(&next) = it.peek() {
                    if next == '/' {
                        it.next();
                        // Skip the comment
                        for _ in it.by_ref() {}
                        tokens.push(Token::Other(" ".to_string()));
                    } else if next == '*' {
                        it.next();
                        // Skip the comment
                        let mut prev = '\0';
                        for c2 in it.by_ref() {
                            if prev == '*' && c2 == '/' {
                                break;
                            }
                            prev = c2;
                            // Skip comment content
                        }
                        tokens.push(Token::Other(" ".to_string()));
                    } else {
                        tokens.push(Token::Other("/".to_string()));
                    }
                } else {
                    tokens.push(Token::Other("/".to_string()));
                }
            } else if ch.is_whitespace() {
                let mut s = String::new();
                while let Some(&c2) = it.peek() {
                    if c2.is_whitespace() {
                        s.push(c2);
                        it.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Other(s));
            } else if let Some(c) = it.next() {
                if c == '#' && it.peek() == Some(&'#') {
                    it.next();
                    tokens.push(Token::Other("##".to_string()));
                } else {
                    tokens.push(Token::Other(c.to_string()));
                }
            } else {
                break;
            }
        }
        tokens
    }

    fn tokens_to_string(tokens: &[Token]) -> String {
        let total_len: usize = tokens.iter().map(|t| Self::token_to_string(t).len()).sum();
        let mut out = String::with_capacity(total_len);
        for t in tokens {
            out.push_str(Self::token_to_string(t));
        }
        out
    }

    /// Convert a token to its string representation for concatenation
    fn token_to_string(token: &Token) -> &str {
        match token {
            Token::Identifier(s)
            | Token::Other(s)
            | Token::StringLiteral(s)
            | Token::CharLiteral(s) => s,
        }
    }

    /// Strip comments from a string, replacing with spaces, but not inside strings
    fn strip_comments(input: &str) -> String {
        if !input.contains('/') {
            return input.to_string();
        }

        let mut result = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        let mut in_string = false;
        let mut quote_char = '\0';

        while let Some(ch) = chars.next() {
            if !in_string {
                if ch == '"' || ch == '\'' {
                    in_string = true;
                    quote_char = ch;
                } else if ch == '/' {
                    if let Some(&'/') = chars.peek() {
                        // Skip // comment
                        chars.next(); // consume second /
                        result.push(' ');
                        for c in chars.by_ref() {
                            if c == '\n' {
                                result.push(c);
                                break;
                            }
                            // Skip comment content
                        }
                        continue;
                    } else if let Some(&'*') = chars.peek() {
                        // Skip /* */ comment
                        chars.next(); // consume *
                        result.push(' ');
                        let mut prev = '\0';
                        for c in chars.by_ref() {
                            if prev == '*' && c == '/' {
                                break;
                            }
                            prev = c;
                            // Skip comment content
                        }
                        continue;
                    }
                }
            } else if ch == quote_char {
                // Check for escape
                let mut backslash_count = 0;
                let mut pos = result.len();
                while pos > 0 && result.as_bytes()[pos - 1] == b'\\' {
                    backslash_count += 1;
                    pos -= 1;
                }
                if backslash_count % 2 == 0 {
                    in_string = false;
                    quote_char = '\0';
                }
            }
            result.push(ch);
        }
        result
    }

    /// Check if a token is whitespace
    fn is_whitespace(token: &Token) -> bool {
        matches!(token, Token::Other(s) if s.chars().all(char::is_whitespace))
    }

    fn trim_token_whitespace(mut tokens: Vec<Token>) -> Vec<Token> {
        let mut start = 0;
        while start < tokens.len() && Self::is_whitespace(&tokens[start]) {
            start += 1;
        }
        let mut end = tokens.len();
        while end > start && Self::is_whitespace(&tokens[end - 1]) {
            end -= 1;
        }
        if start > 0 || end < tokens.len() {
            tokens.drain(end..);
            tokens.drain(0..start);
        }
        tokens
    }

    /// Concatenate two tokens, preserving token type when possible
    fn concatenate_tokens(left: &Token, right: &Token) -> Token {
        let left_str = Self::token_to_string(left);
        let right_str = Self::token_to_string(right);
        let concatenated = format!("{left_str}{right_str}");

        // Result is identifier only if both inputs are identifiers
        match (left, right) {
            (Token::Identifier(_), Token::Identifier(_)) => Token::Identifier(concatenated),
            _ => Token::Other(concatenated),
        }
    }

    /// Apply token pasting (##) to a sequence of tokens
    fn apply_token_pasting(tokens: &[Token]) -> Vec<Token> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            if let Token::Other(s) = &tokens[i]
                && s.trim() == "##"
            {
                // Find previous non-whitespace token
                let mut prev_idx = if result.is_empty() {
                    None
                } else {
                    Some(result.len() - 1)
                };
                while let Some(idx) = prev_idx {
                    if !Self::is_whitespace(&result[idx]) {
                        break;
                    }
                    prev_idx = if idx == 0 { None } else { Some(idx - 1) };
                }
                if let Some(p_idx) = prev_idx {
                    // Pop any whitespace after prev
                    while result.last().is_some_and(Self::is_whitespace) {
                        result.pop();
                    }
                    // Find next non-whitespace token
                    let mut next_idx = i + 1;
                    while next_idx < tokens.len() && Self::is_whitespace(&tokens[next_idx]) {
                        next_idx += 1;
                    }
                    if next_idx < tokens.len() {
                        let concatenated =
                            Self::concatenate_tokens(&result[p_idx], &tokens[next_idx]);
                        result[p_idx] = concatenated;
                        i = next_idx + 1;
                        continue;
                    }
                }
                // If can't find, treat as normal token
                result.push(tokens[i].clone());
            } else {
                result.push(tokens[i].clone());
            }
            i += 1;
        }
        result
    }

    fn expand_tokens(
        &mut self,
        tokens: &[Token],
        depth: usize,
    ) -> Result<Vec<Token>, PreprocessError> {
        if depth > self.recursion_limit {
            return Err(PreprocessError::RecursionLimitExceeded(
                "too deep".to_string(),
            ));
        }

        let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Identifier(name) => {
                    if let Some(token) = self.expand_predefined_macro(name) {
                        out.push(token);
                        i += 1;
                    } else if self.macros.contains_key(name) && !self.disabled_macros.contains(name)
                    {
                        let mac = self.macros[name].clone();
                        i = self.handle_macro_invocation(&mac, name, tokens, i, depth, &mut out)?;
                    } else {
                        out.push(tokens[i].clone());
                        i += 1;
                    }
                }
                _ => {
                    out.push(tokens[i].clone());
                    i += 1;
                }
            }
        }
        Ok(out)
    }

    fn expand_predefined_macro(&self, name: &str) -> Option<Token> {
        match name {
            "__LINE__" => Some(Token::Other(self.current_line.to_string())),
            "__FILE__" => Some(Token::StringLiteral(format!("\"{}\"", self.current_file))),
            "__DATE__" => Some(Token::StringLiteral(format!("\"{}\"", format_date()))),
            "__TIME__" => Some(Token::StringLiteral(format!("\"{}\"", format_time()))),
            _ => None,
        }
    }

    fn handle_macro_invocation(
        &mut self,
        mac: &Macro,
        name: &str,
        tokens: &[Token],
        i: usize,
        depth: usize,
        out: &mut Vec<Token>,
    ) -> Result<usize, PreprocessError> {
        if mac.params.is_some() {
            let next_non_whitespace = self.find_next_non_whitespace(tokens, i + 1);
            let is_function_like_invocation = next_non_whitespace < tokens.len()
                && matches!(&tokens[next_non_whitespace], Token::Other(s) if s.trim_start().starts_with('(') || s == "(");
            if is_function_like_invocation {
                self.handle_function_like_macro(mac, name, tokens, i, depth, out)
            } else {
                self.disabled_macros.insert(name.to_string());
                self.handle_object_like_macro(mac, depth, out)?;
                self.disabled_macros.remove(name);
                Ok(i + 1)
            }
        } else {
            self.disabled_macros.insert(name.to_string());
            self.handle_object_like_macro(mac, depth, out)?;
            self.disabled_macros.remove(name);
            Ok(i + 1)
        }
    }

    fn find_next_non_whitespace(&self, tokens: &[Token], start: usize) -> usize {
        let mut j = start;
        while j < tokens.len() {
            match &tokens[j] {
                Token::Other(s) if s.chars().all(char::is_whitespace) => j += 1,
                _ => break,
            }
        }
        j
    }

    fn handle_object_like_macro(
        &mut self,
        mac: &Macro,
        depth: usize,
        out: &mut Vec<Token>,
    ) -> Result<(), PreprocessError> {
        let pasted = Self::apply_token_pasting(&mac.body);
        let expanded = self.expand_tokens(&pasted, depth + 1)?;
        out.extend(expanded);
        Ok(())
    }

    fn handle_function_like_macro(
        &mut self,
        mac: &Macro,
        name: &str,
        tokens: &[Token],
        i: usize,
        depth: usize,
        out: &mut Vec<Token>,
    ) -> Result<usize, PreprocessError> {
        let paren_token_index = tokens.iter().enumerate().skip(i).find_map(|(k, token)| {
            if let Token::Other(s) = token {
                s.find('(').map(|pos| (k, pos))
            } else {
                None
            }
        });

        if let Some((pt_i, _)) = paren_token_index
            && let Some((args, k)) = self.parse_macro_arguments(tokens, pt_i)?
        {
            if !self.validate_macro_arguments(name, mac, &args)? {
                out.push(tokens[i].clone());
                return Ok(i + 1);
            }

            let replaced = self.replace_macro_parameters(mac, name, &args, depth)?;
            let pasted = Self::apply_token_pasting(&replaced);
            self.disabled_macros.insert(name.to_string());
            let expanded = self.expand_tokens(&pasted, depth + 1)?;
            self.disabled_macros.remove(name);
            out.extend(expanded);
            return Ok(k);
        }

        out.push(tokens[i].clone());
        Ok(i + 1)
    }

    fn parse_macro_arguments(
        &self,
        tokens: &[Token],
        pt_i: usize,
    ) -> Result<Option<MacroParseResult>, PreprocessError> {
        let mut args: MacroArguments = Vec::with_capacity(4);
        let mut current_arg: Vec<Token> = Vec::with_capacity(8);

        let mut paren_balance = 1;
        let mut k = pt_i;
        let mut found_end = false;

        while let Some(t) = tokens.get(k) {
            // Skip the starting token
            if k == pt_i {
                k += 1;
                continue;
            }

            let (is_separator, is_end) = match t {
                Token::Other(s) => {
                    let mut sep = false;
                    let mut end = false;
                    for c in s.chars() {
                        match c {
                            '(' => paren_balance += 1,
                            ')' => {
                                paren_balance -= 1;
                                if paren_balance == 0 {
                                    end = true;
                                }
                            }
                            ',' if paren_balance == 1 => sep = true,
                            _ => {}
                        }
                    }
                    (sep, end)
                }
                _ => (false, false),
            };

            if is_end {
                // push last argument if something was collected
                if !current_arg.is_empty() || !args.is_empty() {
                    args.push(Self::trim_token_whitespace(current_arg));
                }
                found_end = true;
                k += 1;
                break;
            }

            if is_separator {
                args.push(Self::trim_token_whitespace(current_arg));
                current_arg = Vec::with_capacity(8);
            } else {
                current_arg.push(t.clone());
            }

            k += 1;
        }

        if found_end {
            Ok(Some((args, k)))
        } else {
            Ok(None)
        }
    }

    fn validate_macro_arguments(
        &self,
        name: &str,
        mac: &Macro,
        args: &[Vec<Token>],
    ) -> Result<bool, PreprocessError> {
        if let Some(params_list) = &mac.params {
            let min_args = params_list.len();
            let arg_count = args.len();

            if arg_count < min_args {
                return Err(PreprocessError::MacroArgMismatch(format!(
                    "macro {name} expects at least {min_args} args but got {arg_count}",
                )));
            }

            if !mac.is_variadic && arg_count != min_args {
                return Err(PreprocessError::MacroArgMismatch(format!(
                    "macro {name} expects {min_args} args but got {arg_count}",
                )));
            }
        }
        Ok(true)
    }

    fn replace_macro_parameters(
        &mut self,
        mac: &Macro,
        _name: &str,
        args: &[Vec<Token>],
        depth: usize,
    ) -> Result<Vec<Token>, PreprocessError> {
        let params_list = match &mac.params {
            Some(p) => p,
            None => return Ok(mac.body.as_ref().clone()),
        };

        let mut replaced = Vec::with_capacity(mac.body.len());
        let mut body_iter = mac.body.iter().enumerate().peekable();

        // Helpers
        let is_param = |id: &str| params_list.iter().position(|p| p == id);
        let escape_arg = |ts: &[Token]| {
            ts.iter()
                .map(Self::token_to_string)
                .collect::<String>()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        };

        while let Some((_idx, body_t)) = body_iter.next() {
            match body_t {
                // # param stringification
                Token::Other(s) if s.trim() == "#" => {
                    if let Some((_, Token::Identifier(id))) = body_iter.peek()
                        && let Some(pos) = is_param(id)
                    {
                        let escaped = escape_arg(&args[pos]);
                        replaced.push(Token::StringLiteral(format!("\"{escaped}\"")));
                        body_iter.next(); // consume identifier
                        continue;
                    }
                    replaced.push(Token::Other(s.clone()));
                }

                Token::Identifier(id) => {
                    if let Some(pos) = is_param(id) {
                        let expanded = self.expand_tokens(&args[pos], depth + 1)?;
                        replaced.extend(expanded);
                        continue;
                    }

                    if id == "__VA_ARGS__" && mac.is_variadic {
                        let start = params_list.len();
                        for idx in start..args.len() {
                            let expanded = self.expand_tokens(&args[idx], depth + 1)?;
                            replaced.extend(expanded);
                            if idx + 1 < args.len() {
                                replaced.push(Token::Other(",".into()));
                            }
                        }
                        continue;
                    }

                    replaced.push(Token::Identifier(id.clone()));
                }

                other => replaced.push(other.clone()),
            }
        }

        Ok(replaced)
    }

    fn stub_compiler_intrinsics(&mut self) {
        // Common compiler intrinsics that should be defined but not expanded
        let stubs = [
            "__builtin_va_start",
            "__builtin_va_end",
            "__builtin_va_arg",
            "__builtin_offsetof",
            "__builtin_types_compatible_p",
            "__builtin_constant_p",
            "__builtin_expect",
            "__builtin_clz",
            "__builtin_ctz",
            "__builtin_popcount",
            "__builtin_bswap16",
            "__builtin_bswap32",
            "__builtin_bswap64",
        ];

        for stub in stubs {
            self.define_builtin(stub, None, "", false);
        }
    }

    fn define_sizeof_stubs(&mut self) {
        // This would need to know the target, but for simplicity we'll define basic ones
        // In a real implementation, this would be passed the target info
        self.define_builtin("__SIZEOF_INT__", None, "4", false);
        self.define_builtin("__SIZEOF_LONG__", None, "8", false);
        self.define_builtin("__SIZEOF_POINTER__", None, "8", false);
        self.define_builtin("__SIZEOF_LONG_LONG__", None, "8", false);
    }
}
