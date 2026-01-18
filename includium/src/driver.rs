use crate::config::{IncludeContext, IncludeKind, PreprocessorConfig};
use crate::context::{ConditionalState, PreprocessorContext};
use crate::engine::PreprocessorEngine;
use crate::error::PreprocessError;
use crate::macro_def::Macro;
use crate::token::{ExprToken, Token};
use std::collections::HashMap;
use std::rc::Rc;

type MacroArguments = Vec<Vec<Token>>;

/// Public API driver for C preprocessing
///
/// This struct provides the user-facing API for the preprocessor,
/// managing context and delegating to engine for pure operations.
pub struct PreprocessorDriver {
    context: PreprocessorContext,
}

impl Default for PreprocessorDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl PreprocessorDriver {
    /// Create a new preprocessor instance with default configuration
    #[must_use]
    pub fn new() -> Self {
        PreprocessorDriver {
            context: PreprocessorContext::new(),
        }
    }

    /// Create a preprocessor with the given configuration
    #[must_use]
    pub fn with_config(config: &PreprocessorConfig) -> Self {
        let mut driver = Self::new();
        driver.apply_config(config);
        driver
    }

    /// Apply configuration to the preprocessor
    pub fn apply_config(&mut self, config: &PreprocessorConfig) {
        self.context.apply_config(config);
    }

    /// Add a custom include resolver function
    #[must_use]
    pub fn with_include_resolver<F>(mut self, f: F) -> Self
    where
        F: Fn(&str, IncludeKind, &IncludeContext) -> Option<String> + 'static,
    {
        self.context.include_resolver = Some(Rc::new(f));
        self
    }

    /// Set the maximum recursion depth for macro expansion
    pub fn set_recursion_limit(&mut self, limit: usize) {
        self.context.recursion_limit = limit;
    }

    /// Set the current file name for error reporting
    pub fn set_current_file(&mut self, file: String) {
        self.context.current_file = file;
    }

    /// Define a preprocessor macro
    pub fn define<S: AsRef<str>>(
        &mut self,
        name: S,
        params: Option<Vec<String>>,
        body: S,
        is_variadic: bool,
    ) {
        self.context.define(name, params, body, is_variadic);
    }

    /// Remove a macro definition
    pub fn undef(&mut self, name: &str) {
        self.context.undef(name);
    }

    /// Get a reference to the defined macros
    #[must_use]
    pub fn get_macros(&self) -> &HashMap<String, Macro> {
        self.context.get_macros()
    }

    /// Check if a macro is defined
    #[must_use]
    pub fn is_defined(&self, name: &str) -> bool {
        self.context.is_defined(name)
    }

    /// Create a directive error with current location information
    fn directive_error(&self, directive: &str, line: &str) -> PreprocessError {
        let column = Self::calculate_column(line, directive);
        PreprocessError::malformed_directive(
            self.context.current_file.clone(),
            self.context.current_line,
            directive.to_string(),
        )
        .with_column(column)
        .with_source_line(line.to_string())
    }

    /// Create a conditional error with current location information
    fn conditional_error(&self, details: &str, line: &str) -> PreprocessError {
        let column = Self::calculate_column(line, details);
        PreprocessError::conditional_error(
            self.context.current_file.clone(),
            self.context.current_line,
            details.to_owned(),
        )
        .with_column(column)
        .with_source_line(line.to_string())
    }

    /// Create an include error with current location information
    fn include_error(&self, path: &str, line: &str) -> PreprocessError {
        let column = Self::calculate_column(line, path);
        PreprocessError::include_not_found(
            self.context.current_file.clone(),
            self.context.current_line,
            path.to_string(),
        )
        .with_column(column)
        .with_source_line(line.to_string())
    }

    /// Create a generic error with current location information
    fn generic_error(&self, message: &str, line: &str) -> PreprocessError {
        let column = Self::calculate_column(line, message);
        PreprocessError::other(
            self.context.current_file.clone(),
            self.context.current_line,
            message.to_string(),
        )
        .with_column(column)
        .with_source_line(line.to_string())
    }

    /// Calculate the column position of a substring in a line
    fn calculate_column(line: &str, substr: &str) -> usize {
        if substr.is_empty() {
            return 1;
        }
        if let Some(pos) = line.find(substr) {
            return pos + 1;
        }
        line.len() + 1
    }

    /// Process the input C code and return the preprocessed result
    ///
    /// # Errors
    /// Returns `PreprocessError` if there's a malformed directive,
    /// macro recursion limit is exceeded, or conditional blocks are unterminated.
    pub fn process(&mut self, input: &str) -> Result<String, PreprocessError> {
        let spliced = PreprocessorEngine::line_splice(input);
        let pragma_processed = PreprocessorEngine::process_pragma(&spliced);
        let mut out_lines: Vec<String> = Vec::new();
        self.context.conditional_stack.clear();
        self.context.current_line = 1;
        self.context.current_column = 1;

        for current_line_str in pragma_processed.lines() {
            self.context.current_column = 1;
            if let Some(directive) = Self::extract_directive(current_line_str) {
                if let Some(content) = self.handle_directive(directive, current_line_str)? {
                    out_lines.push(content);
                }
            } else if self.can_emit_line() {
                let tokens = PreprocessorEngine::tokenize_line(current_line_str);
                let expanded_tokens = self.expand_tokens(&tokens, 0, current_line_str)?;
                let reconstructed = PreprocessorEngine::tokens_to_string(&expanded_tokens);
                out_lines.push(reconstructed);
            }
            self.context.current_line += 1;
        }

        if !self.context.conditional_stack.is_empty() {
            return Err(self.conditional_error("unterminated #if/#ifdef/#ifndef", "<end of input>"));
        }

        Ok(out_lines.join("\n"))
    }

    /// Checks if the current line should be emitted in the output based on the active
    /// state of conditional compilation directives (#if, #ifdef, #else, etc.).
    fn can_emit_line(&self) -> bool {
        for state in &self.context.conditional_stack {
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
            "define" => self.handle_define(rest, full_line),
            "undef" => self.handle_undef(rest, full_line),
            "include" => self.handle_include(rest, full_line),
            "ifdef" => {
                self.handle_ifdef(rest);
                Ok(None)
            }
            "ifndef" => {
                self.handle_ifndef(rest);
                Ok(None)
            }
            "if" => self.handle_if(rest, full_line),
            "elif" => self.handle_elif(rest, full_line),
            "else" => self.handle_else(full_line),
            "endif" => self.handle_endif(full_line),
            "error" => self.handle_error(rest, full_line),
            "warning" => {
                self.handle_warning(rest);
                Ok(None)
            }
            "line" => self.handle_line(rest, full_line),
            "pragma" => {
                self.handle_pragma(rest);
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn handle_define(
        &mut self,
        rest: &str,
        full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let rest = rest.trim_start();
        if rest.is_empty() {
            return Err(self.directive_error("define", full_line));
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
            return Err(self.directive_error("define", full_line));
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
                    None => return Err(self.directive_error("define", full_line)),
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
        let stripped = PreprocessorEngine::strip_comments(&body_str);
        let stripped_body = stripped.trim();
        let body_tokens = PreprocessorEngine::tokenize_line(stripped_body);
        self.context.macros.insert(
            name,
            Macro {
                params,
                body: Rc::new(body_tokens),
                is_variadic,
                definition_location: Some((
                    self.context.current_file.clone(),
                    self.context.current_line,
                )),
                is_builtin: false,
            },
        );
        Ok(None)
    }

    fn handle_undef(
        &mut self,
        rest: &str,
        full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let name = rest.split_whitespace().next().unwrap_or("");
        if name.is_empty() {
            Err(self.directive_error("undef", full_line))
        } else {
            self.context.undef(name);
            Ok(None)
        }
    }

    fn handle_include(
        &mut self,
        rest: &str,
        full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
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
            return Err(self.directive_error("include", full_line));
        };

        let Some(resolver) = &self.context.include_resolver else {
            return Err(self.include_error(&p, full_line));
        };

        let context = IncludeContext {
            include_stack: self.context.include_stack.clone(),
            include_dirs: Vec::new(),
        };

        let Some(content) = resolver(&p, kind, &context) else {
            return Err(self.include_error(&p, full_line));
        };

        // Check for cycles
        if self.context.include_stack.contains(&p) {
            return Err(
                self.generic_error(&format!("Include cycle detected for '{}'", p), full_line)
            );
        }

        // Check for #pragma once
        if content.contains("#pragma once") && self.context.included_once.contains(&p) {
            return Ok(Some(String::new()));
        }

        self.context
            .include_stack
            .push(self.context.current_file.clone());
        let mut nested = Self {
            context: PreprocessorContext {
                macros: self.context.macros.clone(),
                include_resolver: self.context.include_resolver.clone(),
                recursion_limit: self.context.recursion_limit,
                included_once: self.context.included_once.clone(),
                include_stack: self.context.include_stack.clone(),
                disabled_macros: std::collections::HashSet::new(),
                conditional_stack: Vec::new(),
                current_line: 1,
                current_column: 1,
                current_file: p.clone(),
                compiler: self.context.compiler.clone(),
                warning_handler: self.context.warning_handler.clone(),
            },
        };
        let processed = nested.process(&content)?;
        self.context.include_stack.pop();
        self.context.macros = nested.context.macros;

        if content.contains("#pragma once") {
            self.context.included_once.insert(p);
        }

        Ok(Some(processed))
    }

    fn handle_ifdef(&mut self, rest: &str) {
        let name = rest.trim();
        let defined = self.is_defined(name);
        self.context
            .conditional_stack
            .push(ConditionalState::If(defined));
    }

    fn handle_ifndef(&mut self, rest: &str) {
        let name = rest.trim();
        let defined = self.is_defined(name);
        self.context
            .conditional_stack
            .push(ConditionalState::If(!defined));
    }

    fn handle_if(
        &mut self,
        rest: &str,
        full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
        let evaluated = self.evaluate_expression(rest, full_line)?;
        self.context
            .conditional_stack
            .push(ConditionalState::If(evaluated));
        Ok(None)
    }

    fn handle_elif(
        &mut self,
        rest: &str,
        full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
        if self.context.conditional_stack.is_empty() {
            return Err(self.conditional_error("#elif without #if", full_line));
        }

        let evaluated = self.evaluate_expression(rest, full_line)?;

        if let Some(last) = self.context.conditional_stack.last_mut() {
            *last = ConditionalState::Elif(evaluated);
        }
        Ok(None)
    }

    fn handle_else(&mut self, full_line: &str) -> Result<Option<String>, PreprocessError> {
        let is_active = if let Some(last) = self.context.conditional_stack.last() {
            matches!(
                last,
                ConditionalState::If(false) | ConditionalState::Elif(false)
            )
        } else {
            return Err(self.conditional_error("#else without #if", full_line));
        };

        if let Some(last) = self.context.conditional_stack.last_mut() {
            *last = ConditionalState::Else(is_active);
        }
        Ok(None)
    }

    fn handle_endif(&mut self, full_line: &str) -> Result<Option<String>, PreprocessError> {
        if self.context.conditional_stack.pop().is_none() {
            return Err(self.conditional_error("#endif without #if", full_line));
        }
        Ok(None)
    }

    fn handle_error(
        &mut self,
        rest: &str,
        full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
        if self.can_emit_line() {
            let msg = if rest.is_empty() {
                "#error directive".to_string()
            } else {
                format!("#error: {rest}")
            };
            Err(self.generic_error(&msg, full_line))
        } else {
            Ok(None)
        }
    }

    fn handle_warning(&mut self, rest: &str) {
        if self.can_emit_line()
            && matches!(
                self.context.compiler,
                crate::config::Compiler::GCC | crate::config::Compiler::Clang
            )
        {
            let msg = if rest.is_empty() {
                "#warning directive".to_string()
            } else {
                format!("#warning: {rest}")
            };
            if let Some(ref handler) = self.context.warning_handler {
                handler(&msg);
            }
        }
    }

    fn handle_line(
        &mut self,
        rest: &str,
        full_line: &str,
    ) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.is_empty() {
            return Err(self.directive_error("line", full_line));
        }

        if let Ok(line_num) = parts[0].parse::<usize>() {
            self.context.current_line = line_num.saturating_sub(1);
            self.context.current_column = 1;
            if parts.len() > 1 {
                let filename = parts[1];
                let filename = if let Some(stripped) = filename.strip_prefix('"') {
                    stripped.strip_suffix('"').unwrap_or(stripped)
                } else {
                    filename
                };
                self.context.current_file = filename.to_string();
            }
        }
        Ok(None)
    }

    fn evaluate_expression(
        &mut self,
        expr: &str,
        full_line: &str,
    ) -> Result<bool, PreprocessError> {
        let tokens = PreprocessorEngine::tokenize_line(expr);
        let expanded = self.expand_tokens(&tokens, 0, full_line)?;
        let expr_str = PreprocessorEngine::tokens_to_string(&expanded);
        let trimmed = expr_str.trim();

        if trimmed == "defined" || trimmed.starts_with("defined") {
            let identifier =
                if let (Some(start), Some(end)) = (trimmed.find('('), trimmed.find(')')) {
                    trimmed[start + 1..end].trim()
                } else {
                    trimmed.strip_prefix("defined").unwrap_or(trimmed).trim()
                };
            return Ok(self.is_defined(identifier));
        }

        self.parse_expression(trimmed, full_line)
    }

    fn handle_pragma(&mut self, rest: &str) {
        let trimmed = rest.trim();
        if trimmed == "once" {
            self.context
                .included_once
                .insert(self.context.current_file.clone());
        }
    }

    /// Parse a preprocessor expression with full operator support
    ///
    /// # Errors
    /// Returns `PreprocessError` if the expression is malformed or has invalid operators.
    pub fn parse_expression(
        &mut self,
        expr: &str,
        full_line: &str,
    ) -> Result<bool, PreprocessError> {
        let tokens = PreprocessorEngine::tokenize_expression(expr)?;
        let result = self.evaluate_expression_tokens(&tokens, full_line)?;
        Ok(result != 0)
    }

    fn evaluate_expression_tokens(
        &self,
        tokens: &[ExprToken],
        full_line: &str,
    ) -> Result<i64, PreprocessError> {
        let mut pos = 0;
        let result = self.parse_or(tokens, &mut pos, full_line)?;
        if pos != tokens.len() {
            return Err(self.generic_error("Unexpected tokens at end of expression", full_line));
        }
        Ok(result)
    }

    fn parse_or(
        &self,
        tokens: &[ExprToken],
        pos: &mut usize,
        full_line: &str,
    ) -> Result<i64, PreprocessError> {
        let mut left = self.parse_and(tokens, pos, full_line)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Or => {
                    *pos += 1;
                    let right = self.parse_and(tokens, pos, full_line)?;
                    left = i64::from(left != 0 || right != 0);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_and(
        &self,
        tokens: &[ExprToken],
        pos: &mut usize,
        full_line: &str,
    ) -> Result<i64, PreprocessError> {
        let mut left = self.parse_comparison(tokens, pos, full_line)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::And => {
                    *pos += 1;
                    let right = self.parse_comparison(tokens, pos, full_line)?;
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
        full_line: &str,
    ) -> Result<i64, PreprocessError> {
        let left = self.parse_additive(tokens, pos, full_line)?;
        if *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Equal => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos, full_line)?;
                    return Ok(i64::from(left == right));
                }
                ExprToken::NotEqual => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos, full_line)?;
                    return Ok(i64::from(left != right));
                }
                ExprToken::Less => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos, full_line)?;
                    return Ok(i64::from(left < right));
                }
                ExprToken::LessEqual => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos, full_line)?;
                    return Ok(i64::from(left <= right));
                }
                ExprToken::Greater => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos, full_line)?;
                    return Ok(i64::from(left > right));
                }
                ExprToken::GreaterEqual => {
                    *pos += 1;
                    let right = self.parse_additive(tokens, pos, full_line)?;
                    return Ok(i64::from(left >= right));
                }
                _ => {}
            }
        }
        Ok(left)
    }

    fn parse_additive(
        &self,
        tokens: &[ExprToken],
        pos: &mut usize,
        full_line: &str,
    ) -> Result<i64, PreprocessError> {
        let mut left = self.parse_multiplicative(tokens, pos, full_line)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Plus => {
                    *pos += 1;
                    let right = self.parse_multiplicative(tokens, pos, full_line)?;
                    left += right;
                }
                ExprToken::Minus => {
                    *pos += 1;
                    let right = self.parse_multiplicative(tokens, pos, full_line)?;
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
        full_line: &str,
    ) -> Result<i64, PreprocessError> {
        let mut left = self.parse_unary(tokens, pos, full_line)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Multiply => {
                    *pos += 1;
                    let right = self.parse_unary(tokens, pos, full_line)?;
                    left *= right;
                }
                ExprToken::Divide => {
                    *pos += 1;
                    let right = self.parse_unary(tokens, pos, full_line)?;
                    if right == 0 {
                        return Err(self.generic_error("Division by zero", full_line));
                    }
                    left /= right;
                }
                ExprToken::Modulo => {
                    *pos += 1;
                    let right = self.parse_unary(tokens, pos, full_line)?;
                    if right == 0 {
                        return Err(self.generic_error("Modulo by zero", full_line));
                    }
                    left %= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(
        &self,
        tokens: &[ExprToken],
        pos: &mut usize,
        full_line: &str,
    ) -> Result<i64, PreprocessError> {
        if *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Not => {
                    *pos += 1;
                    let expr = self.parse_unary(tokens, pos, full_line)?;
                    return Ok(i64::from(expr == 0));
                }
                ExprToken::Minus => {
                    *pos += 1;
                    let expr = self.parse_unary(tokens, pos, full_line)?;
                    return Ok(-expr);
                }
                _ => {}
            }
        }
        self.parse_primary(tokens, pos, full_line)
    }

    fn parse_primary(
        &self,
        tokens: &[ExprToken],
        pos: &mut usize,
        full_line: &str,
    ) -> Result<i64, PreprocessError> {
        if *pos >= tokens.len() {
            return Err(self.generic_error("Unexpected end of expression", full_line));
        }

        match &tokens[*pos] {
            ExprToken::Number(val) => {
                *pos += 1;
                Ok(*val)
            }
            ExprToken::Identifier(ident) => {
                *pos += 1;
                if ident == "defined" {
                    if *pos < tokens.len() && matches!(tokens[*pos], ExprToken::LParen) {
                        *pos += 1;
                        if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::Identifier(_))
                        {
                            return Err(
                                self.generic_error("Expected identifier after defined(", full_line)
                            );
                        }
                        if let ExprToken::Identifier(id) = &tokens[*pos] {
                            *pos += 1;
                            if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::RParen) {
                                return Err(self.generic_error(
                                    "Expected ) after defined(identifier",
                                    full_line,
                                ));
                            }
                            *pos += 1;
                            Ok(i64::from(self.is_defined(id)))
                        } else {
                            unreachable!()
                        }
                    } else {
                        Err(self.generic_error(
                            "defined must be followed by identifier or (identifier)",
                            full_line,
                        ))
                    }
                } else {
                    Ok(0)
                }
            }
            ExprToken::LParen => {
                *pos += 1;
                let expr = self.parse_or(tokens, pos, full_line)?;
                if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::RParen) {
                    return Err(self.generic_error("Expected )", full_line));
                }
                *pos += 1;
                Ok(expr)
            }
            _ => Err(self.generic_error("Expected number, identifier, or (", full_line)),
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

    fn expand_tokens(
        &mut self,
        tokens: &[Token],
        depth: usize,
        full_line: &str,
    ) -> Result<Vec<Token>, PreprocessError> {
        if depth > self.context.recursion_limit {
            return Err(PreprocessError::recursion_limit_exceeded(
                self.context.current_file.clone(),
                self.context.current_line,
                "too deep".to_string(),
            )
            .with_source_line(full_line.to_string()));
        }

        let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Identifier(name) => {
                    if let Some(token) =
                        PreprocessorEngine::expand_predefined_macro(&self.context, name)
                    {
                        out.push(token);
                        i += 1;
                    } else if self.context.macros.contains_key(name)
                        && !self.context.disabled_macros.contains(name)
                    {
                        let mac = self.context.macros[name].clone();
                        i = self.handle_macro_invocation(
                            &mac, name, tokens, i, depth, &mut out, full_line,
                        )?;
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

    fn handle_macro_invocation(
        &mut self,
        mac: &Macro,
        name: &str,
        tokens: &[Token],
        i: usize,
        depth: usize,
        out: &mut Vec<Token>,
        full_line: &str,
    ) -> Result<usize, PreprocessError> {
        if mac.params.is_some() {
            let next_non_whitespace = self.find_next_non_whitespace(tokens, i + 1);
            let is_function_like_invocation = next_non_whitespace < tokens.len()
                && matches!(&tokens[next_non_whitespace], Token::Other(s) if s.trim_start().starts_with('(') || s == "(");
            if is_function_like_invocation {
                self.handle_function_like_macro(mac, name, tokens, i, depth, out, full_line)
            } else {
                self.context.disabled_macros.insert(name.to_string());
                self.handle_object_like_macro(mac, depth, out, full_line)?;
                self.context.disabled_macros.remove(name);
                Ok(i + 1)
            }
        } else {
            self.context.disabled_macros.insert(name.to_string());
            self.handle_object_like_macro(mac, depth, out, full_line)?;
            self.context.disabled_macros.remove(name);
            Ok(i + 1)
        }
    }

    fn handle_object_like_macro(
        &mut self,
        mac: &Macro,
        depth: usize,
        out: &mut Vec<Token>,
        full_line: &str,
    ) -> Result<(), PreprocessError> {
        let pasted = PreprocessorEngine::apply_token_pasting(&mac.body);
        let expanded = self.expand_tokens(&pasted, depth + 1, full_line)?;
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
        full_line: &str,
    ) -> Result<usize, PreprocessError> {
        let paren_token_index = tokens.iter().enumerate().skip(i).find_map(|(k, token)| {
            if let Token::Other(s) = token {
                if s.trim().starts_with('(') {
                    Some(k)
                } else {
                    None
                }
            } else {
                None
            }
        });

        let paren_idx = match paren_token_index {
            Some(idx) => idx,
            None => return Ok(i + 1),
        };

        let (args, end_idx) = self.parse_macro_arguments(tokens, paren_idx, mac, full_line)?;

        self.context.disabled_macros.insert(name.to_string());
        let substituted = self.replace_macro_parameters(mac, name, &args, depth + 1, full_line)?;
        self.context.disabled_macros.remove(name);
        let pasted = PreprocessorEngine::apply_token_pasting(&substituted);
        let expanded = self.expand_tokens(&pasted, depth + 1, full_line)?;
        self.context.disabled_macros.insert(name.to_string());
        out.extend(expanded);

        Ok(end_idx)
    }

    fn parse_macro_arguments(
        &mut self,
        tokens: &[Token],
        paren_idx: usize,
        _mac: &Macro,
        full_line: &str,
    ) -> Result<(MacroArguments, usize), PreprocessError> {
        let mut args = Vec::new();
        let mut paren_depth = 1;
        let mut current_arg = Vec::new();
        let mut i = paren_idx + 1;

        while i < tokens.len() {
            match &tokens[i] {
                Token::Other(s) => {
                    for ch in s.chars() {
                        match ch {
                            '(' => paren_depth += 1,
                            ')' => {
                                paren_depth -= 1;
                                if paren_depth == 0 {
                                    args.push(PreprocessorEngine::trim_token_whitespace(
                                        current_arg,
                                    ));
                                    return Ok((args, i + 1));
                                }
                            }
                            ',' => {
                                if paren_depth == 1 {
                                    args.push(PreprocessorEngine::trim_token_whitespace(
                                        current_arg,
                                    ));
                                    current_arg = Vec::new();
                                } else {
                                    current_arg.push(Token::Other(ch.to_string()));
                                }
                            }
                            _ => {
                                current_arg.push(Token::Other(ch.to_string()));
                            }
                        }
                    }
                }
                other => {
                    current_arg.push(other.clone());
                }
            }
            i += 1;
        }

        Err(PreprocessError::macro_arg_mismatch(
            self.context.current_file.clone(),
            self.context.current_line,
            "unterminated macro arguments".to_string(),
        )
        .with_source_line(full_line.to_string()))
    }

    fn replace_macro_parameters(
        &mut self,
        mac: &Macro,
        _name: &str,
        args: &[Vec<Token>],
        depth: usize,
        full_line: &str,
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
                .map(PreprocessorEngine::token_to_string)
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
                        let expanded = self.expand_tokens(&args[pos], depth + 1, full_line)?;
                        replaced.extend(expanded);
                        continue;
                    }

                    if id == "__VA_ARGS__" && mac.is_variadic {
                        let start = params_list.len();
                        for idx in start..args.len() {
                            let expanded = self.expand_tokens(&args[idx], depth + 1, full_line)?;
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
}
