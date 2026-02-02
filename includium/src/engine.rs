use std::iter::Peekable;
use std::str::Chars;

use crate::context::PreprocessorContext;
use crate::error::PreprocessError;
use crate::token::{ExprToken, Token, is_identifier_continue, is_identifier_start};

/// Pure preprocessing engine containing stateless logic
///
/// This struct contains all the pure functions that perform preprocessing
/// operations, making them easy to test and reuse independently of any
/// preprocessor state.
/// Parse an identifier from the character iterator
fn parse_identifier(it: &mut Peekable<Chars>) -> Token {
    let mut s = String::new();
    while let Some(&c) = it.peek() {
        if is_identifier_continue(c) {
            s.push(c);
            it.next();
        } else {
            break;
        }
    }
    Token::Identifier(s)
}

/// Parse a string or character literal from the character iterator
fn parse_literal(it: &mut Peekable<Chars>, quote: char) -> Token {
    let mut s = String::new();
    s.push(quote);
    it.next();

    while let Some(c) = it.next() {
        s.push(c);
        if c == '\\' {
            if let Some(next_char) = it.next() {
                s.push(next_char);
            }
        } else if c == quote {
            break;
        }
    }

    if quote == '"' {
        Token::StringLiteral(s)
    } else {
        Token::CharLiteral(s)
    }
}

/// Parse a comment from the character iterator
fn parse_comment(it: &mut Peekable<Chars>) -> Token {
    it.next(); // Consume the first '/'
    if let Some(&next) = it.peek() {
        if next == '/' {
            it.next();
            // Skip line comment
            for _ in it.by_ref() {}
            return Token::Other(" ".to_string());
        } else if next == '*' {
            it.next();
            // Skip block comment
            let mut prev = '\0';
            for c in it.by_ref() {
                if prev == '*' && c == '/' {
                    break;
                }
                prev = c;
            }
            return Token::Other(" ".to_string());
        }
    }
    Token::Other("/".to_string())
}

/// Parse whitespace from the character iterator
fn parse_whitespace(it: &mut Peekable<Chars>) -> Token {
    let mut s = String::new();
    while let Some(&c) = it.peek() {
        if c.is_whitespace() {
            s.push(c);
            it.next();
        } else {
            break;
        }
    }
    Token::Other(s)
}

/// Tokenize a line of source code into tokens
pub fn tokenize_line(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut it = line.chars().peekable();

    while let Some(&ch) = it.peek() {
        match ch {
            _ if is_identifier_start(ch) => {
                tokens.push(parse_identifier(&mut it));
            }
            '"' | '\'' => {
                tokens.push(parse_literal(&mut it, ch));
            }
            '/' => {
                tokens.push(parse_comment(&mut it));
            }
            _ if ch.is_whitespace() => {
                tokens.push(parse_whitespace(&mut it));
            }
            _ => {
                if let Some(c) = it.next() {
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
        }
    }
    tokens
}

/// Parse a number token from the character iterator
fn parse_number(ch: char, chars: &mut Peekable<Chars>) -> Result<ExprToken, PreprocessError> {
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

    num.parse::<i64>().map(ExprToken::Number).map_err(|_| {
        PreprocessError::other(
            "<expression>".to_string(),
            0,
            format!("Invalid number: {num}"),
        )
    })
}

/// Parse an identifier token from the character iterator
fn parse_expression_identifier(ch: char, chars: &mut Peekable<Chars>) -> ExprToken {
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
    ExprToken::Identifier(ident)
}

/// Parse a two-character operator from the character iterator
fn parse_two_char_operator(
    first: char,
    chars: &mut Peekable<Chars>,
) -> Result<ExprToken, PreprocessError> {
    match first {
        '!' => {
            if let Some(&'=') = chars.peek() {
                chars.next();
                Ok(ExprToken::NotEqual)
            } else {
                Ok(ExprToken::Not)
            }
        }
        '=' => {
            if let Some(&'=') = chars.peek() {
                chars.next();
                Ok(ExprToken::Equal)
            } else {
                Err(PreprocessError::other(
                    "<expression>".to_string(),
                    0,
                    "Invalid operator: =".to_string(),
                ))
            }
        }
        '<' => {
            if let Some(&'=') = chars.peek() {
                chars.next();
                Ok(ExprToken::LessEqual)
            } else if let Some(&'<') = chars.peek() {
                chars.next();
                Ok(ExprToken::ShiftLeft)
            } else {
                Ok(ExprToken::Less)
            }
        }
        '>' => {
            if let Some(&'=') = chars.peek() {
                chars.next();
                Ok(ExprToken::GreaterEqual)
            } else if let Some(&'>') = chars.peek() {
                chars.next();
                Ok(ExprToken::ShiftRight)
            } else {
                Ok(ExprToken::Greater)
            }
        }
        '&' => {
            if let Some(&'&') = chars.peek() {
                chars.next();
                Ok(ExprToken::And)
            } else {
                Ok(ExprToken::BitAnd)
            }
        }
        '|' => {
            if let Some(&'|') = chars.peek() {
                chars.next();
                Ok(ExprToken::Or)
            } else {
                Ok(ExprToken::BitOr)
            }
        }
        _ => Err(PreprocessError::other(
            "<expression>".to_string(),
            0,
            format!("Invalid operator: {first}"),
        )),
    }
}

/// Tokenize expression string into expression tokens
pub fn tokenize_expression(expr: &str) -> Result<Vec<ExprToken>, PreprocessError> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(ch) = chars.next() {
        let token = match ch {
            '0'..='9' => parse_number(ch, &mut chars)?,
            'a'..='z' | 'A'..='Z' | '_' => parse_expression_identifier(ch, &mut chars),
            '(' => ExprToken::LParen,
            ')' => ExprToken::RParen,
            '~' => ExprToken::BitNot,
            '^' => ExprToken::BitXor,
            '+' => ExprToken::Plus,
            '-' => ExprToken::Minus,
            '*' => ExprToken::Multiply,
            '/' => ExprToken::Divide,
            '%' => ExprToken::Modulo,
            c if c.is_whitespace() => continue,
            '!' | '=' | '<' | '>' | '&' | '|' => parse_two_char_operator(ch, &mut chars)?,
            _ => {
                return Err(PreprocessError::other(
                    "<expression>".to_string(),
                    0,
                    format!("Invalid character: {ch}"),
                ));
            }
        };
        tokens.push(token);
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
    let result = parse_or(tokens, &mut pos, &is_defined)?;
    if pos != tokens.len() {
        return Err("Unexpected tokens at end of expression".to_string());
    }
    Ok(result)
}

fn parse_or<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    let mut left = parse_and(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::Or => {
                *pos += 1;
                let right = parse_and(tokens, pos, is_defined)?;
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
    let mut left = parse_bit_or(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::And => {
                *pos += 1;
                let right = parse_bit_or(tokens, pos, is_defined)?;
                left = i64::from(left != 0 && right != 0);
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_bit_or<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    let mut left = parse_bit_xor(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::BitOr => {
                *pos += 1;
                let right = parse_bit_xor(tokens, pos, is_defined)?;
                left |= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_bit_xor<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    let mut left = parse_bit_and(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::BitXor => {
                *pos += 1;
                let right = parse_bit_and(tokens, pos, is_defined)?;
                left ^= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_bit_and<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    let mut left = parse_equality(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::BitAnd => {
                *pos += 1;
                let right = parse_equality(tokens, pos, is_defined)?;
                left &= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_equality<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    let mut left = parse_comparison(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::Equal => {
                *pos += 1;
                let right = parse_comparison(tokens, pos, is_defined)?;
                left = i64::from(left == right);
            }
            ExprToken::NotEqual => {
                *pos += 1;
                let right = parse_comparison(tokens, pos, is_defined)?;
                left = i64::from(left != right);
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_comparison<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    let mut left = parse_shift(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::Less => {
                *pos += 1;
                let right = parse_shift(tokens, pos, is_defined)?;
                left = i64::from(left < right);
            }
            ExprToken::LessEqual => {
                *pos += 1;
                let right = parse_shift(tokens, pos, is_defined)?;
                left = i64::from(left <= right);
            }
            ExprToken::Greater => {
                *pos += 1;
                let right = parse_shift(tokens, pos, is_defined)?;
                left = i64::from(left > right);
            }
            ExprToken::GreaterEqual => {
                *pos += 1;
                let right = parse_shift(tokens, pos, is_defined)?;
                left = i64::from(left >= right);
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_shift<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    let mut left = parse_additive(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::ShiftLeft => {
                *pos += 1;
                let right = parse_additive(tokens, pos, is_defined)?;
                left <<= right;
            }
            ExprToken::ShiftRight => {
                *pos += 1;
                let right = parse_additive(tokens, pos, is_defined)?;
                left >>= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_additive<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    let mut left = parse_multiplicative(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::Plus => {
                *pos += 1;
                let right = parse_multiplicative(tokens, pos, is_defined)?;
                left += right;
            }
            ExprToken::Minus => {
                *pos += 1;
                let right = parse_multiplicative(tokens, pos, is_defined)?;
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
    let mut left = parse_unary(tokens, pos, is_defined)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            ExprToken::Multiply => {
                *pos += 1;
                let right = parse_unary(tokens, pos, is_defined)?;
                left *= right;
            }
            ExprToken::Divide => {
                *pos += 1;
                let right = parse_unary(tokens, pos, is_defined)?;
                if right == 0 {
                    return Err("Division by zero".to_string());
                }
                left /= right;
            }
            ExprToken::Modulo => {
                *pos += 1;
                let right = parse_unary(tokens, pos, is_defined)?;
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
                let expr = parse_unary(tokens, pos, is_defined)?;
                return Ok(i64::from(expr == 0));
            }
            ExprToken::BitNot => {
                *pos += 1;
                let expr = parse_unary(tokens, pos, is_defined)?;
                return Ok(!expr);
            }
            ExprToken::Minus => {
                *pos += 1;
                let expr = parse_unary(tokens, pos, is_defined)?;
                return Ok(-expr);
            }
            ExprToken::Plus => {
                *pos += 1;
                let expr = parse_unary(tokens, pos, is_defined)?;
                return Ok(expr);
            }
            _ => {}
        }
    }
    parse_primary(tokens, pos, is_defined)
}

/// Parse the defined operator: defined identifier or defined(identifier)
fn parse_defined_operator<F>(
    tokens: &[ExprToken],
    pos: &mut usize,
    is_defined: &F,
) -> Result<i64, String>
where
    F: Fn(&str) -> bool,
{
    // Check for defined(identifier) form
    if *pos < tokens.len() && matches!(tokens[*pos], ExprToken::LParen) {
        *pos += 1;

        // Expect identifier after (
        if *pos >= tokens.len() {
            return Err("Expected identifier after defined(".to_string());
        }

        let id = match &tokens[*pos] {
            ExprToken::Identifier(id) => {
                *pos += 1;
                id.clone()
            }
            _ => return Err("Expected identifier after defined(".to_string()),
        };

        // Expect closing )
        if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::RParen) {
            return Err("Expected ) after defined(identifier".to_string());
        }
        *pos += 1;

        Ok(i64::from(is_defined(&id)))
    }
    // Check for defined identifier form
    else if *pos < tokens.len() {
        match &tokens[*pos] {
            ExprToken::Identifier(id) => {
                let defined = is_defined(id);
                *pos += 1;
                Ok(i64::from(defined))
            }
            _ => Err("defined must be followed by identifier or (identifier)".to_string()),
        }
    } else {
        Err("defined must be followed by identifier or (identifier)".to_string())
    }
}

fn parse_primary<F>(tokens: &[ExprToken], pos: &mut usize, is_defined: &F) -> Result<i64, String>
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
                parse_defined_operator(tokens, pos, is_defined)
            } else {
                // Preprocessor treats undefined identifiers as 0
                Ok(0)
            }
        }
        ExprToken::LParen => {
            *pos += 1;
            let val = parse_or(tokens, pos, is_defined)?;
            if *pos >= tokens.len() || !matches!(tokens[*pos], ExprToken::RParen) {
                return Err("Expected )".to_string());
            }
            *pos += 1;
            Ok(val)
        }
        _ => Err("Expected number or identifier".to_string()),
    }
}

/// Check if a string end character is escaped (odd number of backslashes)
fn is_string_end_escaped(result: &str) -> bool {
    let mut backslash_count = 0;
    let mut pos = result.len();
    while pos > 0 && result.as_bytes()[pos - 1] == b'\\' {
        backslash_count += 1;
        pos -= 1;
    }
    backslash_count % 2 == 1
}

/// Handle line comment (//) processing
fn handle_line_comment(chars: &mut Peekable<Chars>, result: &mut String) {
    chars.next(); // Consume second /
    result.push(' ');
    for c in chars.by_ref() {
        if c == '\n' {
            result.push(c);
            break;
        }
    }
}

/// Handle block comment (/* */) processing
fn handle_block_comment(chars: &mut Peekable<Chars>, result: &mut String) {
    chars.next(); // Consume *
    result.push(' ');
    let mut prev = '\0';
    for c in chars.by_ref() {
        if prev == '*' && c == '/' {
            break;
        }
        prev = c;
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
                    handle_line_comment(&mut chars, &mut result);
                    continue;
                } else if let Some(&'*') = chars.peek() {
                    handle_block_comment(&mut chars, &mut result);
                    continue;
                }
            }
        } else if ch == quote_char && !is_string_end_escaped(&result) {
            in_string = false;
            quote_char = '\0';
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
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        let Some(&next) = chars.peek() else {
            out.push(ch);
            continue;
        };

        if next == '\n' {
            chars.next(); // Skip the backslash and newline
            continue;
        }

        if next == '\r' {
            chars.next(); // Skip the backslash and carriage return
            if let Some(&next2) = chars.peek()
                && next2 == '\n'
            {
                chars.next(); // Skip the newline too
            }
            continue;
        }

        out.push(ch);
    }
    out
}

/// Check if we found _Pragma token at position i
fn is_pragma_start(chars: &[char], i: usize) -> bool {
    i + 7 <= chars.len() && chars[i..i + 7] == ['_', 'P', 'r', 'a', 'g', 'm', 'a']
}

/// Skip whitespace to find opening parenthesis
fn find_pragma_paren(chars: &[char], start: usize) -> Option<usize> {
    let mut j = start;
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    if j < chars.len() && chars[j] == '(' {
        Some(j)
    } else {
        None
    }
}

/// Parse the string content inside _Pragma(...)
fn parse_pragma_string(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut j = start;
    if j >= chars.len() || chars[j] != '"' {
        return None;
    }
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
                j += 1;
                return Some((string_content, j));
            } else {
                string_content.push(chars[j]);
            }
        } else {
            string_content.push(chars[j]);
        }
        j += 1;
    }
    None
}

/// Find and consume closing parenthesis
fn consume_pragma_closing_paren(chars: &[char], start: usize) -> Option<usize> {
    let mut j = start;
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    if j < chars.len() && chars[j] == ')' {
        Some(j + 1)
    } else {
        None
    }
}

/// Process a single _Pragma occurrence
fn process_single_pragma(chars: &[char], i: usize, result: &mut String) -> Option<usize> {
    // Find opening parenthesis
    let paren_pos = find_pragma_paren(chars, i + 7)?;
    let string_start = paren_pos + 1;

    // Parse string content
    let (string_content, string_end) = parse_pragma_string(chars, string_start)?;

    // Find closing parenthesis
    let final_pos = consume_pragma_closing_paren(chars, string_end)?;

    // Replace with #pragma
    result.push_str("#pragma ");
    let unescaped = string_content.replace("\\\"", "\"");
    result.push_str(&unescaped);

    Some(final_pos)
}

/// Process _Pragma operators in a line, replacing with #pragma directives
pub fn process_pragma(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();

    while i < chars.len() {
        if is_pragma_start(&chars, i)
            && let Some(new_i) = process_single_pragma(&chars, i, &mut result)
        {
            i = new_i;
            continue;
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
    let total_len: usize = tokens.iter().map(|t| token_to_string(t).len()).sum();
    let mut out = String::with_capacity(total_len);
    for t in tokens {
        out.push_str(token_to_string(t));
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
    while start < tokens.len() && is_whitespace(&tokens[start]) {
        start += 1;
    }
    let mut end = tokens.len();
    while end > start && is_whitespace(&tokens[end - 1]) {
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
    let left_str = token_to_string(left);
    let right_str = token_to_string(right);
    let concatenated = format!("{left_str}{right_str}");

    // Check if result forms a valid identifier
    if is_valid_identifier(&concatenated) {
        Token::Identifier(concatenated)
    } else {
        Token::Other(concatenated)
    }
}

/// Check if a string forms a valid C identifier
fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();

    // First character must be identifier start
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_identifier_start_char(first) {
        return false;
    }

    // All remaining characters must be identifier continue
    chars.all(is_identifier_continue_char)
}

/// Check if character can start an identifier
fn is_identifier_start_char(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

/// Check if character can continue an identifier
fn is_identifier_continue_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// Find the previous non-whitespace token index
fn find_prev_non_whitespace_token(tokens: &[Token], end: usize) -> Option<usize> {
    let mut idx = if end == 0 { return None } else { Some(end - 1) };
    while let Some(current_idx) = idx {
        if !is_whitespace(&tokens[current_idx]) {
            return Some(current_idx);
        }
        idx = if current_idx == 0 {
            None
        } else {
            Some(current_idx - 1)
        };
    }
    None
}

/// Find the next non-whitespace token index
fn find_next_non_whitespace_token(tokens: &[Token], start: usize) -> Option<usize> {
    let mut next_idx = start;
    while next_idx < tokens.len() && is_whitespace(&tokens[next_idx]) {
        next_idx += 1;
    }
    if next_idx < tokens.len() {
        Some(next_idx)
    } else {
        None
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
            // Find previous non-whitespace token in result
            if let Some(p_idx) = find_prev_non_whitespace_token(&result, result.len()) {
                // Pop any whitespace after previous token
                while result.last().is_some_and(is_whitespace) {
                    result.pop();
                }

                // Find next non-whitespace token in input
                if let Some(next_idx) = find_next_non_whitespace_token(tokens, i + 1) {
                    let concatenated = concatenate_tokens(&result[p_idx], &tokens[next_idx]);
                    result[p_idx] = concatenated;
                    i = next_idx + 1;
                    continue;
                }
            }
            // If can't find matching tokens, treat as normal token
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
