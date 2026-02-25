//! Converts AST nodes into `YamlValue` trees, then emits them as YAML text.
//! Handles both compact (default) and verbose output modes.

use crate::ast::*;
use crate::span::Span;

use super::source_map::SourceMapper;
use super::yaml_emitter;
use super::yaml_value::{MappingBuilder, YamlValue};

/// Converts AST nodes into YAML text.
///
/// In compact mode (default), omits absent optional fields and uses inline
/// scalars for single-literal words. Verbose mode includes all fields with
/// explicit null/empty values.
pub struct YamlWriter<'a> {
    mapper: &'a SourceMapper,
    filename: &'a str,
    verbose: bool,
}

impl<'a> YamlWriter<'a> {
    /// Create a compact YAML writer (omits absent optional fields).
    pub fn new(mapper: &'a SourceMapper, filename: &'a str) -> Self {
        YamlWriter {
            mapper,
            filename,
            verbose: false,
        }
    }

    /// Create a verbose YAML writer (includes all fields with explicit null/empty values).
    pub fn new_verbose(mapper: &'a SourceMapper, filename: &'a str) -> Self {
        YamlWriter {
            mapper,
            filename,
            verbose: true,
        }
    }

    /// Render a complete program as YAML text.
    pub fn write_program(&self, prog: &Program) -> String {
        let value = self.build_program(prog);
        yaml_emitter::emit(&value)
    }

    fn source(&self, span: Span) -> String {
        self.mapper.format_span(span, self.filename)
    }

    // AST nodes -------------------------------------------------------------------------------------------------------

    fn build_program(&self, prog: &Program) -> YamlValue {
        let stmts: Vec<YamlValue> = prog.lines.iter().flatten().map(|s| self.build_statement(s)).collect();
        let mut m = MappingBuilder::new();
        m.raw("source", &self.source(prog.span));
        m.value("statements", YamlValue::Sequence(stmts));
        m.build()
    }

    fn emit_mode(&self, m: &mut MappingBuilder, mode: ExecutionMode) {
        match mode {
            ExecutionMode::Background => {
                m.raw("mode", "Background");
            }
            ExecutionMode::Terminated => {
                m.raw("mode", "Terminated");
            }
            ExecutionMode::Sequential => {
                if self.verbose {
                    m.raw("mode", "Sequential");
                }
            }
        }
    }

    fn build_statement(&self, s: &Statement) -> YamlValue {
        let mut m = MappingBuilder::new();
        m.raw("source", &self.source(s.span));
        self.emit_mode(&mut m, s.mode);
        self.extend_expression(&mut m, &s.expression);
        m.build()
    }

    /// Build a sub-mapping for expressions inside binary operator trees
    /// (And/Or/Pipe/Not). These need their own source annotation.
    fn build_inner_expression(&self, expr: &Expression) -> YamlValue {
        use crate::parser::expr_span;
        let mut m = MappingBuilder::new();
        m.raw("source", &self.source(expr_span(expr)));
        self.extend_expression(&mut m, expr);
        m.build()
    }

    fn expression_type_name(expr: &Expression) -> &'static str {
        match expr {
            Expression::Command(_) => "Command",
            Expression::Compound { .. } => "Compound",
            Expression::FunctionDef(_) => "FunctionDef",
            Expression::And { .. } => "And",
            Expression::Or { .. } => "Or",
            Expression::Pipe { .. } => "Pipe",
            Expression::Not(_) => "Not",
        }
    }

    fn extend_expression(&self, m: &mut MappingBuilder, expr: &Expression) {
        m.raw("type", Self::expression_type_name(expr));
        self.extend_expression_body(m, expr);
    }

    fn extend_expression_body(&self, m: &mut MappingBuilder, expr: &Expression) {
        match expr {
            Expression::Command(c) => {
                self.extend_command(m, c);
            }
            Expression::Compound { body, redirects } => {
                m.value("body", self.build_compound_command(body));
                if !redirects.is_empty() {
                    let items: Vec<YamlValue> = redirects.iter().map(|r| self.build_redirect(r)).collect();
                    m.value("redirects", YamlValue::Sequence(items));
                } else if self.verbose {
                    m.empty_seq("redirects");
                }
            }
            Expression::FunctionDef(f) => {
                self.extend_function_def(m, f);
            }
            Expression::And { left, right } => {
                m.value("left", self.build_inner_expression(left));
                m.value("right", self.build_inner_expression(right));
            }
            Expression::Or { left, right } => {
                m.value("left", self.build_inner_expression(left));
                m.value("right", self.build_inner_expression(right));
            }
            Expression::Pipe { left, right, stderr } => {
                if *stderr || self.verbose {
                    m.scalar("stderr", if *stderr { "true" } else { "false" });
                }
                m.value("left", self.build_inner_expression(left));
                m.value("right", self.build_inner_expression(right));
            }
            Expression::Not(inner) => {
                m.value("command", self.build_inner_expression(inner));
            }
        }
    }

    fn extend_command(&self, m: &mut MappingBuilder, cmd: &Command) {
        if !cmd.assignments.is_empty() {
            let items: Vec<YamlValue> = cmd.assignments.iter().map(|a| self.build_assignment(a)).collect();
            m.value("assignments", YamlValue::Sequence(items));
        } else if self.verbose {
            m.empty_seq("assignments");
        }
        let args: Vec<YamlValue> = cmd.arguments.iter().map(|a| self.build_argument(a)).collect();
        m.value("arguments", YamlValue::Sequence(args));
        if !cmd.redirects.is_empty() {
            let items: Vec<YamlValue> = cmd.redirects.iter().map(|r| self.build_redirect(r)).collect();
            m.value("redirects", YamlValue::Sequence(items));
        } else if self.verbose {
            m.empty_seq("redirects");
        }
    }

    fn build_assignment(&self, a: &Assignment) -> YamlValue {
        let mut m = MappingBuilder::new();
        m.raw("source", &self.source(a.span));
        m.scalar("name", &a.name);
        match &a.value {
            AssignmentValue::Scalar(word) => {
                m.value("value", self.build_word_value(word));
            }
            AssignmentValue::BashArray(elements) => {
                m.raw("value_type", "BashArray");
                if !elements.is_empty() {
                    let items: Vec<YamlValue> = elements
                        .iter()
                        .map(|e| match e {
                            ArrayElement::Plain(w) => self.build_word_list_item(w),
                            ArrayElement::Subscripted { index, value } => {
                                let mut em = MappingBuilder::new();
                                em.scalar("index", index);
                                em.value("value", self.build_word_value(value));
                                em.build()
                            }
                        })
                        .collect();
                    m.value("elements", YamlValue::Sequence(items));
                } else if self.verbose {
                    m.empty_seq("elements");
                }
            }
        }
        m.build()
    }

    fn extend_function_def(&self, m: &mut MappingBuilder, f: &FunctionDef) {
        m.raw("source", &self.source(f.span));
        m.scalar("name", &f.name);
        m.value("body", self.build_compound_command(&f.body));
        if !f.redirects.is_empty() {
            let items: Vec<YamlValue> = f.redirects.iter().map(|r| self.build_redirect(r)).collect();
            m.value("redirects", YamlValue::Sequence(items));
        } else if self.verbose {
            m.empty_seq("redirects");
        }
    }

    fn build_compound_command(&self, cmd: &CompoundCommand) -> YamlValue {
        let mut m = MappingBuilder::new();
        match cmd {
            CompoundCommand::BraceGroup { body, span } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "BraceGroup");
                self.extend_lines(&mut m, "body", body);
            }
            CompoundCommand::Subshell { body, span } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "Subshell");
                self.extend_lines(&mut m, "body", body);
            }
            CompoundCommand::ForClause {
                variable,
                words,
                body,
                span,
            } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "ForClause");
                m.scalar("variable", variable);
                if let Some(word_list) = words {
                    let items: Vec<YamlValue> = word_list.iter().map(|w| self.build_word_list_item(w)).collect();
                    m.value("words", YamlValue::Sequence(items));
                } else if self.verbose {
                    m.null("words");
                }
                self.extend_lines(&mut m, "body", body);
            }
            CompoundCommand::CaseClause { word, arms, span } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "CaseClause");
                m.value("word", self.build_word_value(word));
                let items: Vec<YamlValue> = arms.iter().map(|arm| self.build_case_arm(arm)).collect();
                m.value("arms", YamlValue::Sequence(items));
            }
            CompoundCommand::IfClause {
                condition,
                then_body,
                elifs,
                else_body,
                span,
            } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "IfClause");
                self.extend_lines(&mut m, "condition", condition);
                self.extend_lines(&mut m, "then_body", then_body);
                if !elifs.is_empty() {
                    let items: Vec<YamlValue> = elifs.iter().map(|e| self.build_elif(e)).collect();
                    m.value("elifs", YamlValue::Sequence(items));
                } else if self.verbose {
                    m.empty_seq("elifs");
                }
                if let Some(else_cmds) = else_body {
                    self.extend_lines(&mut m, "else_body", else_cmds);
                } else if self.verbose {
                    m.null("else_body");
                }
            }
            CompoundCommand::WhileClause { condition, body, span } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "WhileClause");
                self.extend_lines(&mut m, "condition", condition);
                self.extend_lines(&mut m, "body", body);
            }
            CompoundCommand::UntilClause { condition, body, span } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "UntilClause");
                self.extend_lines(&mut m, "condition", condition);
                self.extend_lines(&mut m, "body", body);
            }
            CompoundCommand::BashDoubleBracket { expression, span } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "BashDoubleBracket");
                m.value("expression", self.build_test_expr(expression));
            }
            CompoundCommand::BashArithmeticCommand { expression, span } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "BashArithmeticCommand");
                m.value("expression", self.build_arith_expr(expression));
            }
            CompoundCommand::BashSelectClause {
                variable,
                words,
                body,
                span,
            } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "BashSelectClause");
                m.scalar("variable", variable);
                if let Some(word_list) = words {
                    let items: Vec<YamlValue> = word_list.iter().map(|w| self.build_word_list_item(w)).collect();
                    m.value("words", YamlValue::Sequence(items));
                } else if self.verbose {
                    m.null("words");
                }
                self.extend_lines(&mut m, "body", body);
            }
            CompoundCommand::BashCoproc { name, body, span } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "BashCoproc");
                if let Some(n) = name {
                    m.scalar("name", n);
                } else if self.verbose {
                    m.null("name");
                }
                let mut inner = MappingBuilder::new();
                self.extend_expression(&mut inner, body);
                m.value("body", inner.build());
            }
            CompoundCommand::BashArithmeticFor {
                init,
                condition,
                update,
                body,
                span,
            } => {
                m.raw("source", &self.source(*span));
                m.raw("type", "BashArithmeticFor");
                if let Some(init_expr) = init {
                    m.value("init", self.build_arith_expr(init_expr));
                } else if self.verbose {
                    m.null("init");
                }
                if let Some(cond_expr) = condition {
                    m.value("condition", self.build_arith_expr(cond_expr));
                } else if self.verbose {
                    m.null("condition");
                }
                if let Some(update_expr) = update {
                    m.value("update", self.build_arith_expr(update_expr));
                } else if self.verbose {
                    m.null("update");
                }
                self.extend_lines(&mut m, "body", body);
            }
        }
        m.build()
    }

    /// Helper: add a named statement list to a mapping, flattening lines.
    fn extend_lines(&self, m: &mut MappingBuilder, key: &str, lines: &[Line]) {
        let items: Vec<YamlValue> = lines.iter().flatten().map(|s| self.build_statement(s)).collect();
        m.value(key, YamlValue::Sequence(items));
    }

    fn build_case_arm(&self, arm: &CaseArm) -> YamlValue {
        let mut m = MappingBuilder::new();
        m.raw("source", &self.source(arm.span));
        let patterns: Vec<YamlValue> = arm.patterns.iter().map(|p| self.build_word_list_item(p)).collect();
        m.value("patterns", YamlValue::Sequence(patterns));
        if !arm.body.is_empty() {
            self.extend_lines(&mut m, "body", &arm.body);
        } else if self.verbose {
            m.empty_seq("body");
        }
        m.build()
    }

    fn build_elif(&self, elif: &ElifClause) -> YamlValue {
        let mut m = MappingBuilder::new();
        m.raw("source", &self.source(elif.span));
        self.extend_lines(&mut m, "condition", &elif.condition);
        self.extend_lines(&mut m, "body", &elif.body);
        m.build()
    }

    fn build_redirect(&self, r: &Redirect) -> YamlValue {
        let type_name = match &r.kind {
            RedirectKind::Input(_) => "Input",
            RedirectKind::Output(_) => "Output",
            RedirectKind::Append(_) => "Append",
            RedirectKind::Clobber(_) => "Clobber",
            RedirectKind::ReadWrite(_) => "ReadWrite",
            RedirectKind::DupInput(_) => "DupInput",
            RedirectKind::DupOutput(_) => "DupOutput",
            RedirectKind::HereDoc { .. } => "HereDoc",
            RedirectKind::BashHereString(_) => "BashHereString",
            RedirectKind::BashOutputAll(_) => "BashOutputAll",
            RedirectKind::BashAppendAll(_) => "BashAppendAll",
        };
        let mut m = MappingBuilder::new();
        m.raw("type", type_name);
        m.raw("source", &self.source(r.span));
        if let Some(fd) = r.fd {
            m.scalar("fd", &fd.to_string());
        } else if self.verbose {
            m.null("fd");
        }
        match &r.kind {
            RedirectKind::HereDoc {
                delimiter,
                body,
                strip_tabs,
                quoted,
                ..
            } => {
                m.scalar("delimiter", delimiter);
                if *strip_tabs || self.verbose {
                    m.scalar("strip_tabs", if *strip_tabs { "true" } else { "false" });
                }
                if *quoted || self.verbose {
                    m.scalar("quoted", if *quoted { "true" } else { "false" });
                }
                m.value("body", YamlValue::BlockScalar(body.clone()));
            }
            _ => {
                let target = match &r.kind {
                    RedirectKind::Input(w)
                    | RedirectKind::Output(w)
                    | RedirectKind::Append(w)
                    | RedirectKind::Clobber(w)
                    | RedirectKind::ReadWrite(w)
                    | RedirectKind::DupInput(w)
                    | RedirectKind::DupOutput(w)
                    | RedirectKind::BashHereString(w)
                    | RedirectKind::BashOutputAll(w)
                    | RedirectKind::BashAppendAll(w) => w,
                    _ => unreachable!(),
                };
                m.value("target", self.build_word_value(target));
            }
        }
        m.build()
    }

    /// Build a word as a sequence item. Single-literal words use compact scalar form
    /// in normal mode; verbose mode always uses the full `parts` form.
    fn build_word_list_item(&self, word: &Word) -> YamlValue {
        if !self.verbose && word.parts.len() == 1 {
            if let Fragment::Literal(s) = &word.parts[0] {
                return YamlValue::scalar(s.clone());
            }
        }
        let mut m = MappingBuilder::new();
        m.raw("source", &self.source(word.span));
        let parts: Vec<YamlValue> = word.parts.iter().map(|p| self.build_fragment(p)).collect();
        m.value("parts", YamlValue::Sequence(parts));
        m.build()
    }

    fn build_argument(&self, arg: &Argument) -> YamlValue {
        match arg {
            Argument::Word(w) => self.build_word_list_item(w),
            Argument::Atom(atom) => self.build_atom(atom),
        }
    }

    fn build_atom(&self, atom: &Atom) -> YamlValue {
        match atom {
            Atom::BashProcessSubstitution { direction, body, span } => {
                let dir = match direction {
                    ProcessDirection::In => "<",
                    ProcessDirection::Out => ">",
                };
                let mut m = MappingBuilder::new();
                m.raw("type", "BashProcessSubstitution");
                m.raw("source", &self.source(*span));
                m.scalar("direction", dir);
                if !body.is_empty() {
                    let stmts: Vec<YamlValue> = body.iter().map(|s| self.build_inner_statement(s)).collect();
                    m.value("statements", YamlValue::Sequence(stmts));
                } else if self.verbose {
                    m.empty_seq("statements");
                }
                m.build()
            }
        }
    }

    /// Build a word as an inline mapping value (not a sequence item).
    /// Single-literal words use compact `literal: value` form in normal mode;
    /// verbose mode always uses the full `parts` form.
    fn build_word_value(&self, word: &Word) -> YamlValue {
        if !self.verbose && word.parts.len() == 1 {
            if let Fragment::Literal(s) = &word.parts[0] {
                let mut m = MappingBuilder::new();
                m.scalar("literal", s);
                return m.build();
            }
        }
        let mut m = MappingBuilder::new();
        m.raw("source", &self.source(word.span));
        let parts: Vec<YamlValue> = word.parts.iter().map(|p| self.build_fragment(p)).collect();
        m.value("parts", YamlValue::Sequence(parts));
        m.build()
    }

    fn build_fragment(&self, part: &Fragment) -> YamlValue {
        let mut m = MappingBuilder::new();
        match part {
            Fragment::Literal(s) => {
                m.raw("type", "Literal");
                m.scalar("value", s);
            }
            Fragment::SingleQuoted(s) => {
                m.raw("type", "SingleQuoted");
                m.scalar("value", s);
            }
            Fragment::DoubleQuoted(parts) => {
                m.raw("type", "DoubleQuoted");
                let items: Vec<YamlValue> = parts.iter().map(|p| self.build_fragment(p)).collect();
                m.value("parts", YamlValue::Sequence(items));
            }
            Fragment::Parameter(expansion) => {
                m.raw("type", "Parameter");
                self.extend_param_expansion(&mut m, expansion);
            }
            Fragment::CommandSubstitution(stmts) => {
                m.raw("type", "CommandSubstitution");
                if !stmts.is_empty() {
                    let items: Vec<YamlValue> = stmts.iter().map(|s| self.build_inner_statement(s)).collect();
                    m.value("statements", YamlValue::Sequence(items));
                } else if self.verbose {
                    m.empty_seq("statements");
                }
            }
            Fragment::ArithmeticExpansion(expr) => {
                m.raw("type", "ArithmeticExpansion");
                m.value("expression", self.build_arith_expr(expr));
            }
            Fragment::Glob(g) => {
                let g_str = match g {
                    GlobChar::Star => "Star",
                    GlobChar::Question => "Question",
                    GlobChar::BracketOpen => "BracketOpen",
                };
                m.raw("type", "Glob");
                m.raw("value", g_str);
            }
            Fragment::TildePrefix(s) => {
                m.raw("type", "TildePrefix");
                if s.is_empty() {
                    m.scalar("value", "~");
                } else {
                    m.scalar("value", &format!("~{}", s));
                }
            }
            Fragment::BashAnsiCQuoted(s) => {
                m.raw("type", "BashAnsiCQuoted");
                m.scalar("value", s);
            }
            Fragment::BashLocaleQuoted { parts, .. } => {
                m.raw("type", "BashLocaleQuoted");
                let items: Vec<YamlValue> = parts.iter().map(|p| self.build_fragment(p)).collect();
                m.value("parts", YamlValue::Sequence(items));
            }
            Fragment::BashBraceExpansion(brace) => {
                m.raw("type", "BashBraceExpansion");
                match brace {
                    BraceExpansionKind::List(items) => {
                        m.raw("kind", "list");
                        let yaml_items: Vec<YamlValue> = items
                            .iter()
                            .map(|item| {
                                if !self.verbose && item.len() == 1 {
                                    if let Fragment::Literal(s) = &item[0] {
                                        return YamlValue::scalar(s.clone());
                                    }
                                }
                                let parts: Vec<YamlValue> = item.iter().map(|p| self.build_fragment(p)).collect();
                                let mut im = MappingBuilder::new();
                                im.value("parts", YamlValue::Sequence(parts));
                                im.build()
                            })
                            .collect();
                        m.value("items", YamlValue::Sequence(yaml_items));
                    }
                    BraceExpansionKind::Sequence { start, end, step } => {
                        m.raw("kind", "sequence");
                        m.scalar("start", start);
                        m.scalar("end", end);
                        if let Some(s) = step {
                            m.scalar("step", s);
                        } else if self.verbose {
                            m.null("step");
                        }
                    }
                }
            }
            Fragment::BashExtGlob { kind, pattern } => {
                let kind_str = match kind {
                    ExtGlobKind::ZeroOrOne => "?",
                    ExtGlobKind::ZeroOrMore => "*",
                    ExtGlobKind::OneOrMore => "+",
                    ExtGlobKind::ExactlyOne => "@",
                    ExtGlobKind::Not => "!",
                };
                m.raw("type", "BashExtGlob");
                m.scalar("kind", kind_str);
                m.scalar("pattern", pattern);
            }
        }
        m.build()
    }

    /// Build a statement inside a command substitution or process substitution.
    /// These don't get source annotations (inner offsets are relative).
    fn build_inner_statement(&self, s: &Statement) -> YamlValue {
        let mut m = MappingBuilder::new();
        self.emit_mode(&mut m, s.mode);
        self.extend_expression(&mut m, &s.expression);
        m.build()
    }

    fn build_test_expr(&self, expr: &BashTestExpr) -> YamlValue {
        let mut m = MappingBuilder::new();
        match expr {
            BashTestExpr::Unary { op, arg } => {
                m.raw("type", "Unary");
                m.scalar("op", Self::unary_test_op_str(*op));
                m.value("arg", self.build_word_value(arg));
            }
            BashTestExpr::Binary { left, op, right } => {
                m.raw("type", "Binary");
                m.scalar("op", Self::binary_test_op_str(*op));
                m.value("left", self.build_word_value(left));
                m.value("right", self.build_word_value(right));
            }
            BashTestExpr::And { left, right } => {
                m.raw("type", "And");
                m.value("left", self.build_test_expr(left));
                m.value("right", self.build_test_expr(right));
            }
            BashTestExpr::Or { left, right } => {
                m.raw("type", "Or");
                m.value("left", self.build_test_expr(left));
                m.value("right", self.build_test_expr(right));
            }
            BashTestExpr::Not(inner) => {
                m.raw("type", "Not");
                m.value("expr", self.build_test_expr(inner));
            }
            BashTestExpr::Group(inner) => {
                m.raw("type", "Group");
                m.value("expr", self.build_test_expr(inner));
            }
            BashTestExpr::Word(w) => {
                m.raw("type", "Word");
                m.value("value", self.build_word_value(w));
            }
        }
        m.build()
    }

    fn unary_test_op_str(op: UnaryTestOp) -> &'static str {
        match op {
            UnaryTestOp::FileExists => "-e",
            UnaryTestOp::FileIsRegular => "-f",
            UnaryTestOp::FileIsDirectory => "-d",
            UnaryTestOp::FileIsSymlink => "-L",
            UnaryTestOp::FileIsBlockDev => "-b",
            UnaryTestOp::FileIsCharDev => "-c",
            UnaryTestOp::FileIsPipe => "-p",
            UnaryTestOp::FileIsSocket => "-S",
            UnaryTestOp::FileHasSize => "-s",
            UnaryTestOp::FileDescriptorOpen => "-t",
            UnaryTestOp::FileIsReadable => "-r",
            UnaryTestOp::FileIsWritable => "-w",
            UnaryTestOp::FileIsExecutable => "-x",
            UnaryTestOp::FileIsSetuid => "-u",
            UnaryTestOp::FileIsSetgid => "-g",
            UnaryTestOp::FileIsSticky => "-k",
            UnaryTestOp::FileIsOwnedByUser => "-O",
            UnaryTestOp::FileIsOwnedByGroup => "-G",
            UnaryTestOp::FileModifiedSinceRead => "-N",
            UnaryTestOp::StringIsEmpty => "-z",
            UnaryTestOp::StringIsNonEmpty => "-n",
            UnaryTestOp::VariableIsSet => "-v",
            UnaryTestOp::VariableIsNameRef => "-R",
        }
    }

    fn binary_test_op_str(op: BinaryTestOp) -> &'static str {
        match op {
            BinaryTestOp::StringEquals => "==",
            BinaryTestOp::StringNotEquals => "!=",
            BinaryTestOp::StringLessThan => "<",
            BinaryTestOp::StringGreaterThan => ">",
            BinaryTestOp::RegexMatch => "=~",
            BinaryTestOp::IntEq => "-eq",
            BinaryTestOp::IntNe => "-ne",
            BinaryTestOp::IntLt => "-lt",
            BinaryTestOp::IntLe => "-le",
            BinaryTestOp::IntGt => "-gt",
            BinaryTestOp::IntGe => "-ge",
            BinaryTestOp::FileNewerThan => "-nt",
            BinaryTestOp::FileOlderThan => "-ot",
            BinaryTestOp::FileSameDevice => "-ef",
        }
    }

    fn build_arith_expr(&self, expr: &ArithExpr) -> YamlValue {
        let mut m = MappingBuilder::new();
        match expr {
            ArithExpr::Number(n) => {
                m.raw("type", "Number");
                m.scalar("value", &n.to_string());
            }
            ArithExpr::Variable(s) => {
                m.raw("type", "Variable");
                m.scalar("value", s);
            }
            ArithExpr::Binary { left, op, right } => {
                let op_str = match op {
                    ArithBinaryOp::Add => "+",
                    ArithBinaryOp::Sub => "-",
                    ArithBinaryOp::Mul => "*",
                    ArithBinaryOp::Div => "/",
                    ArithBinaryOp::Mod => "%",
                    ArithBinaryOp::Exp => "**",
                    ArithBinaryOp::ShiftLeft => "<<",
                    ArithBinaryOp::ShiftRight => ">>",
                    ArithBinaryOp::BitAnd => "&",
                    ArithBinaryOp::BitOr => "|",
                    ArithBinaryOp::BitXor => "^",
                    ArithBinaryOp::LogAnd => "&&",
                    ArithBinaryOp::LogOr => "||",
                    ArithBinaryOp::Eq => "==",
                    ArithBinaryOp::Ne => "!=",
                    ArithBinaryOp::Lt => "<",
                    ArithBinaryOp::Le => "<=",
                    ArithBinaryOp::Gt => ">",
                    ArithBinaryOp::Ge => ">=",
                };
                m.raw("type", "Binary");
                m.scalar("op", op_str);
                m.value("left", self.build_arith_expr(left));
                m.value("right", self.build_arith_expr(right));
            }
            ArithExpr::UnaryPrefix { op, operand } => {
                m.raw("type", "UnaryPrefix");
                m.scalar("op", Self::arith_unary_op_str(*op));
                m.value("operand", self.build_arith_expr(operand));
            }
            ArithExpr::UnaryPostfix { operand, op } => {
                m.raw("type", "UnaryPostfix");
                m.scalar("op", Self::arith_unary_op_str(*op));
                m.value("operand", self.build_arith_expr(operand));
            }
            ArithExpr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                m.raw("type", "Ternary");
                m.value("condition", self.build_arith_expr(condition));
                m.value("then", self.build_arith_expr(then_expr));
                m.value("else", self.build_arith_expr(else_expr));
            }
            ArithExpr::Assignment { target, op, value } => {
                let op_str = match op {
                    ArithAssignOp::Assign => "=",
                    ArithAssignOp::AddAssign => "+=",
                    ArithAssignOp::SubAssign => "-=",
                    ArithAssignOp::MulAssign => "*=",
                    ArithAssignOp::DivAssign => "/=",
                    ArithAssignOp::ModAssign => "%=",
                    ArithAssignOp::ShiftLeftAssign => "<<=",
                    ArithAssignOp::ShiftRightAssign => ">>=",
                    ArithAssignOp::BitAndAssign => "&=",
                    ArithAssignOp::BitOrAssign => "|=",
                    ArithAssignOp::BitXorAssign => "^=",
                };
                m.raw("type", "Assignment");
                m.scalar("target", target);
                m.scalar("op", op_str);
                m.value("value", self.build_arith_expr(value));
            }
            ArithExpr::Group(inner) => {
                m.raw("type", "Group");
                m.value("expr", self.build_arith_expr(inner));
            }
            ArithExpr::Comma { left, right } => {
                m.raw("type", "Comma");
                m.value("left", self.build_arith_expr(left));
                m.value("right", self.build_arith_expr(right));
            }
        }
        m.build()
    }

    fn arith_unary_op_str(op: ArithUnaryOp) -> &'static str {
        match op {
            ArithUnaryOp::Negate => "-",
            ArithUnaryOp::Plus => "+",
            ArithUnaryOp::LogNot => "!",
            ArithUnaryOp::BitNot => "~",
            ArithUnaryOp::Increment => "++",
            ArithUnaryOp::Decrement => "--",
        }
    }

    fn extend_param_expansion(&self, m: &mut MappingBuilder, exp: &ParameterExpansion) {
        match exp {
            ParameterExpansion::Simple(name) => {
                m.scalar("name", &format!("${}", name));
            }
            ParameterExpansion::Complex {
                name,
                indirect,
                operator,
                argument,
            } => {
                m.scalar("name", name);
                if *indirect {
                    m.scalar("indirect", "true");
                } else if self.verbose {
                    m.scalar("indirect", "false");
                }
                if let Some(op) = operator {
                    let op_str = match op {
                        ParamOp::Default => ":-",
                        ParamOp::DefaultAssign => ":=",
                        ParamOp::Error => ":?",
                        ParamOp::Alternative => ":+",
                        ParamOp::Length => "#",
                        ParamOp::TrimSmallSuffix => "%",
                        ParamOp::TrimLargeSuffix => "%%",
                        ParamOp::TrimSmallPrefix => "#",
                        ParamOp::TrimLargePrefix => "##",
                        ParamOp::UpperFirst => "^",
                        ParamOp::UpperAll => "^^",
                        ParamOp::LowerFirst => ",",
                        ParamOp::LowerAll => ",,",
                        ParamOp::TransformQuote => "@Q",
                        ParamOp::TransformEscape => "@E",
                        ParamOp::TransformPrompt => "@P",
                        ParamOp::TransformAssignment => "@A",
                        ParamOp::TransformAttributes => "@a",
                        ParamOp::TransformLower => "@L",
                        ParamOp::TransformUpper => "@U",
                        ParamOp::TransformCapitalize => "@u",
                        ParamOp::TransformKeyValue => "@K",
                        ParamOp::TransformKeys => "@k",
                    };
                    m.scalar("operator", op_str);
                } else if self.verbose {
                    m.null("operator");
                }
                if let Some(arg) = argument {
                    m.value("argument", self.build_word_value(arg));
                } else if self.verbose {
                    m.null("argument");
                }
            }
        }
    }
}
