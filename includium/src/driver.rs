use crate::config::{IncludeContext, IncludeKind, PreprocessorConfig};
use crate::context::{ConditionalState, PreprocessorContext};
use crate::engine;
use crate::error::PreprocessError;
use crate::macro_def::Macro;
use crate::token::{ExprToken, Token};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

type MacroArguments = Vec<Vec<Token>>;

/// Context for error diagnostics, bundling location information
#[derive(Clone, Debug)]
pub struct DiagnosticContext {
    /// Source file name
    pub file: String,
    /// Line number (1-based)
    pub line: usize,
    /// Optional source line content for display
    pub source_line: Option<String>,
}

impl DiagnosticContext {
    /// Create a new diagnostic context
    pub fn new(file: String, line: usize, source_line: Option<String>) -> Self {
        Self {
            file,
            line,
            source_line,
        }
    }
}

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

    /// Create a directive error with location information
    fn directive_error(&self, directive: &str, ctx: &DiagnosticContext) -> PreprocessError {
        let column = ctx
            .source_line
            .as_ref()
            .map_or(1, |line| Self::calculate_column(line, directive));
        let mut error =
            PreprocessError::malformed_directive(ctx.file.clone(), ctx.line, directive.to_string())
                .with_column(column);
        if let Some(ref source) = ctx.source_line {
            error = error.with_source_line(source.clone());
        }
        error
    }

    /// Create a conditional error with location information
    fn conditional_error(&self, details: &str, ctx: &DiagnosticContext) -> PreprocessError {
        let column = ctx
            .source_line
            .as_ref()
            .map_or(1, |line| Self::calculate_column(line, details));
        let mut error =
            PreprocessError::conditional_error(ctx.file.clone(), ctx.line, details.to_owned())
                .with_column(column);
        if let Some(ref source) = ctx.source_line {
            error = error.with_source_line(source.clone());
        }
        error
    }

    /// Create a generic error with location information
    fn generic_error(&self, message: &str, ctx: &DiagnosticContext) -> PreprocessError {
        let column = ctx
            .source_line
            .as_ref()
            .map_or(1, |line| Self::calculate_column(line, message));
        let mut error = PreprocessError::other(ctx.file.clone(), ctx.line, message.to_string())
            .with_column(column);
        if let Some(ref source) = ctx.source_line {
            error = error.with_source_line(source.clone());
        }
        error
    }

    /// Create an include error with location information
    fn include_error(&self, path: &str, ctx: &DiagnosticContext) -> PreprocessError {
        let column = ctx
            .source_line
            .as_ref()
            .map_or(1, |line| Self::calculate_column(line, path));
        let mut error =
            PreprocessError::include_not_found(ctx.file.clone(), ctx.line, path.to_string())
                .with_column(column);
        if let Some(ref source) = ctx.source_line {
            error = error.with_source_line(source.clone());
        }
        error
    }

    /// Calculate the character-based column position of a substring in a line
    ///
    /// Returns the 1-based character index where the substring starts.
    /// This is best-effort positioning and may not be exact for repeated substrings.
    fn calculate_column(line: &str, substr: &str) -> usize {
        if substr.is_empty() {
            return 1;
        }
        if let Some(byte_pos) = line.find(substr) {
            // Convert byte position to character position
            return line[..byte_pos].chars().count() + 1;
        }
        // If not found, position at end of line
        line.chars().count() + 1
    }

    /// Process the input C code and return the preprocessed result
    ///
    /// # Errors
    /// Returns `PreprocessError` if there's a malformed directive,
    /// macro recursion limit is exceeded, or conditional blocks are unterminated.
    pub fn process(&mut self, input: &str) -> Result<String, PreprocessError> {
        let spliced = engine::line_splice(input);
        let pragma_processed = engine::process_pragma(&spliced);
        let mut out_lines: Vec<String> = Vec::new();
        self.context.conditional_stack.clear();
        self.context.current_line = 1;

        for current_line_str in pragma_processed.lines() {
            let stripped_line = engine::strip_comments(current_line_str);
            let ctx = DiagnosticContext::new(
                self.context.current_file.clone(),
                self.context.current_line,
                Some(current_line_str.to_string()),
            );

            if let Some(directive) = Self::extract_directive(&stripped_line) {
                if let Some(content) = self.handle_directive(directive, &ctx)? {
                    out_lines.push(content);
                }
            } else if self.can_emit_line() {
                let tokens = engine::tokenize_line(&stripped_line);
                let expanded_tokens = self.expand_tokens(&tokens, 0, &ctx)?;
                let reconstructed = engine::tokens_to_string(&expanded_tokens);
                out_lines.push(reconstructed);
            }
            self.context.current_line += 1;
        }

        if !self.context.conditional_stack.is_empty() {
            let ctx = DiagnosticContext::new("<end of input>".to_string(), 0, None);
            return Err(self.conditional_error("unterminated #if/#ifdef/#ifndef", &ctx));
        }

        Ok(out_lines.join("\n") + "\n")
    }

    /// Checks if the current line should be emitted in the output based on the active
    /// state of conditional compilation directives (#if, #ifdef, #else, etc.).
    fn can_emit_line(&self) -> bool {
        for state in &self.context.conditional_stack {
            if !state.is_active {
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
        ctx: &DiagnosticContext,
    ) -> Result<Option<String>, PreprocessError> {
        let mut parts = directive.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").trim();
        let rest = parts.next().unwrap_or("").trim();

        match cmd {
            "define" => self.handle_define(rest, ctx),
            "undef" => self.handle_undef(rest, ctx),
            "include" => self.handle_include(rest, ctx),
            "ifdef" => {
                self.handle_ifdef(rest);
                Ok(None)
            }
            "ifndef" => {
                self.handle_ifndef(rest);
                Ok(None)
            }
            "if" => self.handle_if(rest, ctx),
            "elif" => self.handle_elif(rest, ctx),
            "else" => self.handle_else(ctx),
            "endif" => self.handle_endif(ctx),
            "error" => self.handle_error(rest, ctx),
            "warning" => {
                self.handle_warning(rest, ctx);
                Ok(None)
            }
            "line" => self.handle_line(rest, ctx),
            "pragma" => Ok(self.handle_pragma(rest)),
            _ => Ok(None),
        }
    }

    fn handle_define(
        &mut self,
        rest: &str,
        ctx: &DiagnosticContext,
    ) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let rest = rest.trim_start();
        if rest.is_empty() {
            return Err(self.directive_error("define", ctx));
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
            return Err(self.directive_error("define", ctx));
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
                    None => return Err(self.directive_error("define", ctx)),
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
        let stripped = engine::strip_comments(&body_str);
        let stripped_body = stripped.trim();
        let body_tokens = engine::tokenize_line(stripped_body);
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
        ctx: &DiagnosticContext,
    ) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let name = rest.split_whitespace().next().unwrap_or("");
        if name.is_empty() {
            Err(self.directive_error("undef", ctx))
        } else {
            self.context.undef(name);
            Ok(None)
        }
    }

    fn handle_include(
        &mut self,
        rest: &str,
        ctx: &DiagnosticContext,
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
            return Err(self.directive_error("include", ctx));
        };

        let context = IncludeContext {
            include_stack: self.context.include_stack.clone(),
            include_dirs: Vec::new(),
        };

        let Some(resolver) = &self.context.include_resolver else {
            return Err(self.include_error(&p, ctx));
        };

        let Some(content) = resolver(&p, kind.clone(), &context) else {
            return Err(self.include_error(&p, ctx));
        };

        // Check for cycles
        if self.context.include_stack.contains(&p) {
            return Err(self.generic_error(&format!("Include cycle detected for '{}'", p), ctx));
        }

        // Check for #pragma once
        if content.contains("#pragma once") && self.context.included_once.contains(&p) {
            return Ok(Some(String::new()));
        }

        // Push current file to include stack BEFORE resolving path for proper context
        self.context
            .include_stack
            .push(self.context.current_file.clone());

        // For local includes, try to resolve the actual file path
        // This ensures __FILE__ shows the correct relative path
        let resolved_path = if kind == IncludeKind::Local {
            self.context
                .include_stack
                .last()
                .and_then(|including_file| Path::new(including_file).parent())
                .map(|parent_dir| parent_dir.join(&p))
                .filter(|candidate| candidate.exists())
                .map(|candidate| candidate.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone())
        } else {
            p.clone()
        };

        let mut nested = Self {
            context: PreprocessorContext {
                macros: self.context.macros.clone(),
                include_resolver: self.context.include_resolver.clone(),
                recursion_limit: self.context.recursion_limit,
                included_once: self.context.included_once.clone(),
                include_stack: self.context.include_stack.clone(),
                disabled_macros: HashSet::new(),
                conditional_stack: Vec::new(),
                current_line: 1,
                current_file: resolved_path,
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
            .push(ConditionalState::new(defined));
    }

    fn handle_ifndef(&mut self, rest: &str) {
        let name = rest.trim();
        let defined = self.is_defined(name);
        self.context
            .conditional_stack
            .push(ConditionalState::new(!defined));
    }

    fn handle_if(
        &mut self,
        rest: &str,
        ctx: &DiagnosticContext,
    ) -> Result<Option<String>, PreprocessError> {
        let evaluated = self.evaluate_expression(rest, ctx)?;
        self.context
            .conditional_stack
            .push(ConditionalState::new(evaluated));
        Ok(None)
    }

    fn handle_elif(
        &mut self,
        rest: &str,
        ctx: &DiagnosticContext,
    ) -> Result<Option<String>, PreprocessError> {
        if self.context.conditional_stack.is_empty() {
            return Err(self.conditional_error("#elif without #if", ctx));
        }

        let (already_taken, outer_active) = {
            let last = self.context.conditional_stack.last().unwrap();
            let outer_active = self
                .context
                .conditional_stack
                .iter()
                .rev()
                .skip(1)
                .all(|s| s.is_active);
            (last.any_branch_taken, outer_active)
        };

        if already_taken || !outer_active {
            if let Some(last) = self.context.conditional_stack.last_mut() {
                last.is_active = false;
            }
        } else {
            let evaluated = self.evaluate_expression(rest, ctx)?;
            if let Some(last) = self.context.conditional_stack.last_mut() {
                last.is_active = evaluated;
                if evaluated {
                    last.any_branch_taken = true;
                }
            }
        }
        Ok(None)
    }

    fn handle_else(&mut self, ctx: &DiagnosticContext) -> Result<Option<String>, PreprocessError> {
        if self.context.conditional_stack.is_empty() {
            return Err(self.conditional_error("#else without #if", ctx));
        }

        let (already_taken, outer_active) = {
            let last = self.context.conditional_stack.last().unwrap();
            let outer_active = self
                .context
                .conditional_stack
                .iter()
                .rev()
                .skip(1)
                .all(|s| s.is_active);
            (last.any_branch_taken, outer_active)
        };

        if let Some(last) = self.context.conditional_stack.last_mut() {
            last.is_active = !already_taken && outer_active;
            last.any_branch_taken = true; // No more branches after else
        }
        Ok(None)
    }

    fn handle_endif(&mut self, ctx: &DiagnosticContext) -> Result<Option<String>, PreprocessError> {
        if self.context.conditional_stack.pop().is_none() {
            return Err(self.conditional_error("#endif without #if", ctx));
        }
        Ok(None)
    }

    fn handle_error(
        &mut self,
        rest: &str,
        ctx: &DiagnosticContext,
    ) -> Result<Option<String>, PreprocessError> {
        if self.can_emit_line() {
            let msg = if rest.is_empty() {
                "#error directive".to_string()
            } else {
                format!("#error: {rest}")
            };
            Err(self.generic_error(&msg, ctx))
        } else {
            Ok(None)
        }
    }

    fn handle_warning(&mut self, rest: &str, _ctx: &DiagnosticContext) {
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
        ctx: &DiagnosticContext,
    ) -> Result<Option<String>, PreprocessError> {
        if !self.can_emit_line() {
            return Ok(None);
        }

        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.is_empty() {
            return Err(self.directive_error("line", ctx));
        }

        if let Ok(line_num) = parts[0].parse::<usize>() {
            self.context.current_line = line_num.saturating_sub(1);
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
        ctx: &DiagnosticContext,
    ) -> Result<bool, PreprocessError> {
        let tokens = engine::tokenize_line(expr);
        let expanded = self.expand_tokens(&tokens, 0, ctx)?;
        let expr_str = engine::tokens_to_string(&expanded);
        let trimmed = expr_str.trim();

        self.parse_expression(trimmed, ctx)
    }

    fn handle_pragma(&mut self, rest: &str) -> Option<String> {
        let trimmed = rest.trim();
        if trimmed == "once" {
            self.context
                .included_once
                .insert(self.context.current_file.clone());
            None
        } else {
            Some(format!("#pragma {rest}"))
        }
    }

    /// Parse a preprocessor expression with full operator support
    ///
    /// # Errors
    /// Returns `PreprocessError` if the expression is malformed or has invalid operators.
    pub fn parse_expression(
        &mut self,
        expr: &str,
        ctx: &DiagnosticContext,
    ) -> Result<bool, PreprocessError> {
        let tokens = engine::tokenize_expression(expr)?;
        let result = self.evaluate_expression_tokens(&tokens, ctx)?;
        Ok(result != 0)
    }

    fn evaluate_expression_tokens(
        &self,
        tokens: &[ExprToken],
        ctx: &DiagnosticContext,
    ) -> Result<i64, PreprocessError> {
        let result = engine::evaluate_expression_tokens(tokens, |id| self.is_defined(id));
        match result {
            Ok(val) => Ok(val),
            Err(msg) => Err(self.generic_error(&msg, ctx)),
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
        ctx: &DiagnosticContext,
    ) -> Result<Vec<Token>, PreprocessError> {
        if depth > self.context.recursion_limit {
            return Err(PreprocessError::recursion_limit_exceeded(
                self.context.current_file.clone(),
                self.context.current_line,
                "too deep".to_string(),
            )
            .with_source_line(ctx.source_line.clone().unwrap_or_default()));
        }

        let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Identifier(name) => {
                    if name == "defined" {
                        out.push(tokens[i].clone());
                        i += 1;
                        // Skip whitespace
                        while i < tokens.len()
                            && matches!(&tokens[i], Token::Other(s) if s.chars().all(char::is_whitespace))
                        {
                            out.push(tokens[i].clone());
                            i += 1;
                        }
                        if i < tokens.len() {
                            if let Token::Other(s) = &tokens[i]
                                && s == "("
                            {
                                out.push(tokens[i].clone());
                                i += 1;
                                // Skip whitespace
                                while i < tokens.len()
                                    && matches!(&tokens[i], Token::Other(s) if s.chars().all(char::is_whitespace))
                                {
                                    out.push(tokens[i].clone());
                                    i += 1;
                                }
                                if i < tokens.len() {
                                    out.push(tokens[i].clone()); // The identifier
                                    i += 1;
                                }
                                // Skip whitespace
                                while i < tokens.len()
                                    && matches!(&tokens[i], Token::Other(s) if s.chars().all(char::is_whitespace))
                                {
                                    out.push(tokens[i].clone());
                                    i += 1;
                                }
                                if i < tokens.len()
                                    && matches!(&tokens[i], Token::Other(s) if s == ")")
                                {
                                    out.push(tokens[i].clone());
                                    i += 1;
                                }
                            } else {
                                out.push(tokens[i].clone()); // The identifier
                                i += 1;
                            }
                        }
                        continue;
                    }

                    if let Some(token) = engine::expand_predefined_macro(&self.context, name) {
                        out.push(token);
                        i += 1;
                    } else if self.context.macros.contains_key(name)
                        && !self.context.disabled_macros.contains(name)
                    {
                        let mac = self.context.macros[name].clone();
                        i = self
                            .handle_macro_invocation(&mac, name, tokens, i, depth, &mut out, ctx)?;
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
        ctx: &DiagnosticContext,
    ) -> Result<usize, PreprocessError> {
        if mac.params.is_some() {
            let next_non_whitespace = self.find_next_non_whitespace(tokens, i + 1);
            let is_function_like_invocation = next_non_whitespace < tokens.len()
                && matches!(&tokens[next_non_whitespace], Token::Other(s) if s.trim_start().starts_with('(') || s == "(");
            if is_function_like_invocation {
                self.handle_function_like_macro(mac, name, tokens, i, depth, out, ctx)
            } else {
                // Function-like macro without ( is not expanded
                out.push(Token::Identifier(name.to_string()));
                Ok(i + 1)
            }
        } else {
            self.context.disabled_macros.insert(name.to_string());
            let result = self.handle_object_like_macro(mac, depth, out, ctx);
            self.context.disabled_macros.remove(name);
            result?;
            Ok(i + 1)
        }
    }

    fn handle_object_like_macro(
        &mut self,
        mac: &Macro,
        depth: usize,
        out: &mut Vec<Token>,
        ctx: &DiagnosticContext,
    ) -> Result<(), PreprocessError> {
        let pasted = engine::apply_token_pasting(&mac.body);
        let expanded = self.expand_tokens(&pasted, depth + 1, ctx)?;
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
        ctx: &DiagnosticContext,
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

        let (args, end_idx) = self.parse_macro_arguments(tokens, paren_idx, mac, ctx)?;

        let substituted = self.replace_macro_parameters(mac, name, &args, depth + 1, ctx)?;

        self.context.disabled_macros.insert(name.to_string());
        let pasted = engine::apply_token_pasting(&substituted);
        let expanded_res = self.expand_tokens(&pasted, depth + 1, ctx);
        self.context.disabled_macros.remove(name);

        let expanded_tokens = expanded_res?;
        out.extend(expanded_tokens);

        Ok(end_idx)
    }

    fn parse_macro_arguments(
        &mut self,
        tokens: &[Token],
        paren_idx: usize,
        _mac: &Macro,
        ctx: &DiagnosticContext,
    ) -> Result<(MacroArguments, usize), PreprocessError> {
        let mut args = Vec::new();
        let mut paren_depth = 1;
        let mut current_arg = Vec::new();
        let mut i = paren_idx + 1;

        while i < tokens.len() {
            match &tokens[i] {
                Token::Other(s) => {
                    // Check if this token contains special characters that need to be processed individually
                    if s.contains(['(', ')', ',']) {
                        for ch in s.chars() {
                            match ch {
                                '(' => {
                                    paren_depth += 1;
                                    current_arg.push(Token::Other("(".to_string()));
                                }
                                ')' => {
                                    paren_depth -= 1;
                                    if paren_depth == 0 {
                                        args.push(engine::trim_token_whitespace(current_arg));
                                        return Ok((args, i + 1));
                                    }
                                    current_arg.push(Token::Other(")".to_string()));
                                }
                                ',' => {
                                    if paren_depth == 1 {
                                        args.push(engine::trim_token_whitespace(current_arg));
                                        current_arg = Vec::new();
                                    } else {
                                        current_arg.push(Token::Other(",".to_string()));
                                    }
                                }
                                _ => {
                                    current_arg.push(Token::Other(ch.to_string()));
                                }
                            }
                        }
                    } else {
                        // Token doesn't contain special characters, treat as single unit
                        current_arg.push(Token::Other(s.clone()));
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
        .with_source_line(ctx.source_line.clone().unwrap_or_default()))
    }

    fn replace_macro_parameters(
        &mut self,
        mac: &Macro,
        _name: &str,
        args: &[Vec<Token>],
        depth: usize,
        ctx: &DiagnosticContext,
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
                .map(engine::token_to_string)
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
                        let expanded = self.expand_tokens(&args[pos], depth + 1, ctx)?;
                        replaced.extend(expanded);
                        continue;
                    }

                    if id == "__VA_ARGS__" && mac.is_variadic {
                        let start = params_list.len();
                        for idx in start..args.len() {
                            let expanded = self.expand_tokens(&args[idx], depth + 1, ctx)?;
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
