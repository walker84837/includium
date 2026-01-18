use crate::context::PreprocessorContext;
use crate::error::PreprocessError;
use crate::token::{ExprToken, Token, is_identifier_continue, is_identifier_start};

/// Pure preprocessing engine containing stateless logic
///
/// This struct contains all the pure functions that perform preprocessing
/// operations, making them easy to test and reuse independently of any
/// preprocessor state.
pub struct PreprocessorEngine;

impl PreprocessorEngine {
    /// Tokenize a line of source code into tokens
    pub fn tokenize_line(line: &str) -> Vec<Token> {
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

    /// Tokenize expression string into expression tokens
    pub fn tokenize_expression(expr: &str) -> Result<Vec<ExprToken>, PreprocessError> {
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
                        return Err(PreprocessError::other(
                            "<expression>".to_string(),
                            0,
                            format!("Invalid number: {num}"),
                        ));
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
                        return Err(PreprocessError::other(
                            "<expression>".to_string(),
                            0,
                            "Invalid operator: =".to_string(),
                        ));
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
                        return Err(PreprocessError::other(
                            "<expression>".to_string(),
                            0,
                            "Invalid operator: &".to_string(),
                        ));
                    }
                }
                '|' => {
                    if let Some(&'|') = chars.peek() {
                        chars.next();
                        tokens.push(ExprToken::Or);
                    } else {
                        return Err(PreprocessError::other(
                            "<expression>".to_string(),
                            0,
                            "Invalid operator: |".to_string(),
                        ));
                    }
                }
                '+' => tokens.push(ExprToken::Plus),
                '-' => tokens.push(ExprToken::Minus),
                '*' => tokens.push(ExprToken::Multiply),
                '/' => tokens.push(ExprToken::Divide),
                '%' => tokens.push(ExprToken::Modulo),
                c if c.is_whitespace() => {}
                _ => {
                    return Err(PreprocessError::other(
                        "<expression>".to_string(),
                        0,
                        format!("Invalid character: {ch}"),
                    ));
                }
            }
        }

        Ok(tokens)
    }

    /// Evaluate a preprocessor expression from tokens
    ///
    /// # Errors
    /// Returns an error message if the expression is malformed.
    pub fn evaluate_expression_tokens<F>(tokens: &[ExprToken], is_defined: F) -> Result<i64, String>
    where
        F: Fn(&str) -> bool,
    {
        let mut pos = 0;
        let result = Self::parse_or(tokens, &mut pos, &is_defined)?;
        if pos != tokens.len() {
            return Err("Unexpected tokens at end of expression".to_string());
        }
        Ok(result)
    }

    fn parse_or<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
    where
        F: Fn(&str) -> bool,
    {
        let mut left = Self::parse_and(tokens, pos, is_defined)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Or => {
                    *pos += 1;
                    let right = Self::parse_and(tokens, pos, is_defined)?;
                    left = i64::from(left != 0 || right != 0);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_and<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
    where
        F: Fn(&str) -> bool,
    {
        let mut left = Self::parse_comparison(tokens, pos, is_defined)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::And => {
                    *pos += 1;
                    let right = Self::parse_comparison(tokens, pos, is_defined)?;
                    left = i64::from(left != 0 && right != 0);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_comparison<F>(
        tokens: &[ExprToken],
        pos: &mut usize,
        is_defined: &F,
    ) -> Result<i64, String>
    where
        F: Fn(&str) -> bool,
    {
        let left = Self::parse_additive(tokens, pos, is_defined)?;
        if *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Equal => {
                    *pos += 1;
                    let right = Self::parse_additive(tokens, pos, is_defined)?;
                    return Ok(i64::from(left == right));
                }
                ExprToken::NotEqual => {
                    *pos += 1;
                    let right = Self::parse_additive(tokens, pos, is_defined)?;
                    return Ok(i64::from(left != right));
                }
                ExprToken::Less => {
                    *pos += 1;
                    let right = Self::parse_additive(tokens, pos, is_defined)?;
                    return Ok(i64::from(left < right));
                }
                ExprToken::LessEqual => {
                    *pos += 1;
                    let right = Self::parse_additive(tokens, pos, is_defined)?;
                    return Ok(i64::from(left <= right));
                }
                ExprToken::Greater => {
                    *pos += 1;
                    let right = Self::parse_additive(tokens, pos, is_defined)?;
                    return Ok(i64::from(left > right));
                }
                ExprToken::GreaterEqual => {
                    *pos += 1;
                    let right = Self::parse_additive(tokens, pos, is_defined)?;
                    return Ok(i64::from(left >= right));
                }
                _ => {}
            }
        }
        Ok(left)
    }

    fn parse_additive<F>(
        tokens: &[ExprToken],
        pos: &mut usize,
        is_defined: &F,
    ) -> Result<i64, String>
    where
        F: Fn(&str) -> bool,
    {
        let mut left = Self::parse_multiplicative(tokens, pos, is_defined)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Plus => {
                    *pos += 1;
                    let right = Self::parse_multiplicative(tokens, pos, is_defined)?;
                    left += right;
                }
                ExprToken::Minus => {
                    *pos += 1;
                    let right = Self::parse_multiplicative(tokens, pos, is_defined)?;
                    left -= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplicative<F>(
        tokens: &[ExprToken],
        pos: &mut usize,
        is_defined: &F,
    ) -> Result<i64, String>
    where
        F: Fn(&str) -> bool,
    {
        let mut left = Self::parse_unary(tokens, pos, is_defined)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Multiply => {
                    *pos += 1;
                    let right = Self::parse_unary(tokens, pos, is_defined)?;
                    left *= right;
                }
                ExprToken::Divide => {
                    *pos += 1;
                    let right = Self::parse_unary(tokens, pos, is_defined)?;
                    if right == 0 {
                        return Err("Division by zero".to_string());
                    }
                    left /= right;
                }
                ExprToken::Modulo => {
                    *pos += 1;
                    let right = Self::parse_unary(tokens, pos, is_defined)?;
                    if right == 0 {
                        return Err("Modulo by zero".to_string());
                    }
                    left %= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
    where
        F: Fn(&str) -> bool,
    {
        if *pos < tokens.len() {
            match tokens[*pos] {
                ExprToken::Not => {
                    *pos += 1;
                    let expr = Self::parse_unary(tokens, pos, is_defined)?;
                    return Ok(i64::from(expr == 0));
                }
                ExprToken::Minus => {
                    *pos += 1;
                    let expr = Self::parse_unary(tokens, pos, is_defined)?;
                    return Ok(-expr);
                }
                _ => {}
            }
        }
        Self::parse_primary(tokens, pos, is_defined)
    }

    fn parse_primary<F>(
        tokens: &[ExprToken],
        pos: &mut usize,
        is_defined: &F,
    ) -> Result<i64, String>
    where
        F: Fn(&str) -> bool,
    {
        if *pos >= tokens.len() {
            return Err("Unexpected end of expression".to_string());
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
                            return Err("Expected identifier after defined(".to_string());
                        }
                        if let ExprToken::Identifier(id) = &tokens[*pos] {
                            *pos += 1;
                            if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::RParen) {
                                return Err("Expected ) after defined(identifier".to_string());
                            }
                            *pos += 1;
                            Ok(i64::from(is_defined(id)))
                        } else {
                            unreachable!()
                        }
                    } else if *pos < tokens.len() {
                        if let ExprToken::Identifier(id) = &tokens[*pos] {
                            let defined = is_defined(id);
                            *pos += 1;
                            return Ok(i64::from(defined));
                        }
                        Err("defined must be followed by identifier or (identifier)".to_string())
                    } else {
                        Err("defined must be followed by identifier or (identifier)".to_string())
                    }
                } else {
                    // Preprocessor treats undefined identifiers as 0
                    Ok(0)
                }
            }
            ExprToken::LParen => {
                *pos += 1;
                let val = Self::parse_or(tokens, pos, is_defined)?;
                if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::RParen) {
                    return Err("Expected )".to_string());
                }
                *pos += 1;
                Ok(val)
            }
            _ => Err("Expected number or identifier".to_string()),
        }
    }

    /// Strip comments from a string, replacing with spaces, but not inside strings
    pub fn strip_comments(input: &str) -> String {
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
                        chars.next();
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
                        chars.next();
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

    /// Perform line splicing (join lines ending with backslash)
    pub fn line_splice(input: &str) -> String {
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
    pub fn process_pragma(line: &str) -> String {
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

    /// Convert a token to its string representation for concatenation
    pub fn token_to_string(token: &Token) -> &str {
        match token {
            Token::Identifier(s)
            | Token::Other(s)
            | Token::StringLiteral(s)
            | Token::CharLiteral(s) => s,
        }
    }

    /// Convert tokens back to a string
    pub fn tokens_to_string(tokens: &[Token]) -> String {
        let total_len: usize = tokens.iter().map(|t| Self::token_to_string(t).len()).sum();
        let mut out = String::with_capacity(total_len);
        for t in tokens {
            out.push_str(Self::token_to_string(t));
        }
        out
    }

    /// Check if a token is whitespace
    fn is_whitespace(token: &Token) -> bool {
        matches!(token, Token::Other(s) if s.chars().all(char::is_whitespace))
    }

    /// Trim whitespace tokens from the beginning and end of a token sequence
    pub fn trim_token_whitespace(mut tokens: Vec<Token>) -> Vec<Token> {
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
    pub fn apply_token_pasting(tokens: &[Token]) -> Vec<Token> {
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

    /// Expand predefined macros (__LINE__, __FILE__, __DATE__, __TIME__)
    pub fn expand_predefined_macro(context: &PreprocessorContext, name: &str) -> Option<Token> {
        use crate::date_time::{format_date, format_time};

        match name {
            "__LINE__" => Some(Token::Other(context.current_line.to_string())),
            "__FILE__" => Some(Token::StringLiteral(format!(
                "\"{}\"",
                context.current_file
            ))),
            "__DATE__" => Some(Token::StringLiteral(format!("\"{}\"", format_date()))),
            "__TIME__" => Some(Token::StringLiteral(format!("\"{}\"", format_time()))),
            _ => None,
        }
    }
}
