use shell_parser::ast::*;
use shell_parser::span::Span;
use std::fmt::Write as FmtWrite;

use super::source_map::SourceMapper;

pub(super) struct YamlWriter<'a> {
    mapper: &'a SourceMapper,
    filename: &'a str,
    buf: String,
}

impl<'a> YamlWriter<'a> {
    pub(super) fn new(mapper: &'a SourceMapper, filename: &'a str) -> Self {
        YamlWriter {
            mapper,
            filename,
            buf: String::new(),
        }
    }

    pub(super) fn finish(self) -> String {
        self.buf
    }

    fn source(&self, span: Span) -> String {
        self.mapper.format_span(span, self.filename)
    }

    // --- Primitives ---

    fn indent(&mut self, level: usize) {
        for _ in 0..level {
            self.buf.push(' ');
        }
    }

    fn key_value(&mut self, level: usize, key: &str, value: &str) {
        self.indent(level);
        let _ = writeln!(self.buf, "{}: {}", key, value);
    }

    fn key_block(&mut self, level: usize, key: &str) {
        self.indent(level);
        let _ = writeln!(self.buf, "{}:", key);
    }

    fn key_source(&mut self, level: usize, span: Span) {
        self.key_value(level, "source", &self.source(span));
    }

    // --- AST nodes ---

    pub(super) fn write_program(&mut self, prog: &Program) {
        self.key_source(0, prog.span);
        self.key_block(0, "statements");
        for s in &prog.statements {
            self.write_statement(2, s);
        }
    }

    fn write_statement(&mut self, level: usize, s: &Statement) {
        self.indent(level);
        self.buf.push_str("- ");
        let src = self.source(s.span);
        let _ = writeln!(self.buf, "source: {}", src);
        match s.mode {
            ExecutionMode::Background => {
                self.key_value(level + 2, "mode", "Background");
            }
            ExecutionMode::Terminated => {
                self.key_value(level + 2, "mode", "Terminated");
            }
            ExecutionMode::Sequential => {}
        }
        self.write_expression(level + 2, &s.expression);
    }

    /// Write an expression that appears inside a binary operator tree
    /// (And/Or/Pipe/Not). These need their own source annotation since
    /// they don't have a Statement wrapper.
    fn write_inner_expression(&mut self, level: usize, expr: &Expression) {
        use shell_parser::parser::expr_span;
        self.key_source(level, expr_span(expr));
        self.write_expression(level, expr);
    }

    /// Write an expression inline after a `- ` list prefix (first key
    /// appears on the same line as `- `, no leading indent).
    fn write_expression_inline(&mut self, level: usize, expr: &Expression) {
        let type_name = match expr {
            Expression::Command(_) => "Command",
            Expression::Compound { .. } => "Compound",
            Expression::FunctionDef(_) => "FunctionDef",
            Expression::And { .. } => "And",
            Expression::Or { .. } => "Or",
            Expression::Pipe { .. } => "Pipe",
            Expression::Not(_) => "Not",
        };
        let _ = writeln!(self.buf, "type: {}", type_name);
        self.write_expression_body(level + 2, expr);
    }

    fn write_expression(&mut self, level: usize, expr: &Expression) {
        let type_name = match expr {
            Expression::Command(_) => "Command",
            Expression::Compound { .. } => "Compound",
            Expression::FunctionDef(_) => "FunctionDef",
            Expression::And { .. } => "And",
            Expression::Or { .. } => "Or",
            Expression::Pipe { .. } => "Pipe",
            Expression::Not(_) => "Not",
        };
        self.key_value(level, "type", type_name);
        self.write_expression_body(level, expr);
    }

    fn write_expression_body(&mut self, level: usize, expr: &Expression) {
        match expr {
            Expression::Command(c) => {
                self.write_command(level, c);
            }
            Expression::Compound { body, redirects } => {
                self.key_block(level, "body");
                self.write_compound_command(level + 2, body);
                if !redirects.is_empty() {
                    self.key_block(level, "redirects");
                    for r in redirects {
                        self.write_redirect(level + 2, r);
                    }
                }
            }
            Expression::FunctionDef(f) => {
                self.write_function_def(level, f);
            }
            Expression::And { left, right } => {
                self.key_block(level, "left");
                self.write_inner_expression(level + 2, left);
                self.key_block(level, "right");
                self.write_inner_expression(level + 2, right);
            }
            Expression::Or { left, right } => {
                self.key_block(level, "left");
                self.write_inner_expression(level + 2, left);
                self.key_block(level, "right");
                self.write_inner_expression(level + 2, right);
            }
            Expression::Pipe {
                left,
                right,
                stderr,
            } => {
                if *stderr {
                    self.key_value(level, "stderr", "true");
                }
                self.key_block(level, "left");
                self.write_inner_expression(level + 2, left);
                self.key_block(level, "right");
                self.write_inner_expression(level + 2, right);
            }
            Expression::Not(inner) => {
                self.key_block(level, "command");
                self.write_inner_expression(level + 2, inner);
            }
        }
    }

    fn write_command(&mut self, level: usize, cmd: &Command) {
        if !cmd.assignments.is_empty() {
            self.key_block(level, "assignments");
            for a in &cmd.assignments {
                self.write_assignment(level + 2, a);
            }
        }
        self.key_block(level, "arguments");
        for arg in &cmd.arguments {
            self.write_argument(level + 2, arg);
        }
        if !cmd.redirects.is_empty() {
            self.key_block(level, "redirects");
            for r in &cmd.redirects {
                self.write_redirect(level + 2, r);
            }
        }
    }

    fn write_assignment(&mut self, level: usize, a: &Assignment) {
        self.indent(level);
        let src = self.source(a.span);
        let _ = writeln!(self.buf, "- source: {}", src);
        self.key_value(level + 2, "name", &a.name);
        match &a.value {
            AssignmentValue::Scalar(word) => {
                self.key_block(level + 2, "value");
                self.write_word_inline(level + 4, word);
            }
            AssignmentValue::BashArray(elements) => {
                self.key_value(level + 2, "value_type", "BashArray");
                if !elements.is_empty() {
                    self.key_block(level + 2, "elements");
                    for w in elements {
                        self.write_word_list_item(level + 4, w);
                    }
                }
            }
        }
    }

    fn write_function_def(&mut self, level: usize, f: &FunctionDef) {
        self.key_source(level, f.span);
        self.key_value(level, "name", &f.name);
        self.key_block(level, "body");
        self.write_compound_command(level + 2, &f.body);
        if !f.redirects.is_empty() {
            self.key_block(level, "redirects");
            for r in &f.redirects {
                self.write_redirect(level + 2, r);
            }
        }
    }

    fn write_compound_command(&mut self, level: usize, cmd: &CompoundCommand) {
        match cmd {
            CompoundCommand::BraceGroup { body, span } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "BraceGroup");
                self.key_block(level, "body");
                for c in body {
                    self.write_statement(level + 2, c);
                }
            }
            CompoundCommand::Subshell { body, span } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "Subshell");
                self.key_block(level, "body");
                for c in body {
                    self.write_statement(level + 2, c);
                }
            }
            CompoundCommand::ForClause {
                variable,
                words,
                body,
                span,
            } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "ForClause");
                self.key_value(level, "variable", variable);
                if let Some(word_list) = words {
                    self.key_block(level, "words");
                    for w in word_list {
                        self.write_word_list_item(level + 2, w);
                    }
                }
                self.key_block(level, "body");
                for c in body {
                    self.write_statement(level + 2, c);
                }
            }
            CompoundCommand::CaseClause { word, arms, span } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "CaseClause");
                self.key_block(level, "word");
                self.write_word_inline(level + 2, word);
                self.key_block(level, "arms");
                for arm in arms {
                    self.write_case_arm(level + 2, arm);
                }
            }
            CompoundCommand::IfClause {
                condition,
                then_body,
                elifs,
                else_body,
                span,
            } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "IfClause");
                self.key_block(level, "condition");
                for c in condition {
                    self.write_statement(level + 2, c);
                }
                self.key_block(level, "then_body");
                for c in then_body {
                    self.write_statement(level + 2, c);
                }
                if !elifs.is_empty() {
                    self.key_block(level, "elifs");
                    for elif in elifs {
                        self.write_elif(level + 2, elif);
                    }
                }
                if let Some(else_cmds) = else_body {
                    self.key_block(level, "else_body");
                    for c in else_cmds {
                        self.write_statement(level + 2, c);
                    }
                }
            }
            CompoundCommand::WhileClause {
                condition,
                body,
                span,
            } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "WhileClause");
                self.key_block(level, "condition");
                for c in condition {
                    self.write_statement(level + 2, c);
                }
                self.key_block(level, "body");
                for c in body {
                    self.write_statement(level + 2, c);
                }
            }
            CompoundCommand::UntilClause {
                condition,
                body,
                span,
            } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "UntilClause");
                self.key_block(level, "condition");
                for c in condition {
                    self.write_statement(level + 2, c);
                }
                self.key_block(level, "body");
                for c in body {
                    self.write_statement(level + 2, c);
                }
            }
            CompoundCommand::BashDoubleBracket { expression, span } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "BashDoubleBracket");
                self.key_block(level, "expression");
                self.write_test_expr(level + 2, expression);
            }
            CompoundCommand::BashArithmeticCommand { expression, span } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "BashArithmeticCommand");
                self.key_block(level, "expression");
                self.write_arith_expr(level + 2, expression);
            }
            CompoundCommand::BashSelectClause {
                variable,
                words,
                body,
                span,
            } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "BashSelectClause");
                self.key_value(level, "variable", variable);
                if let Some(word_list) = words {
                    self.key_block(level, "words");
                    for w in word_list {
                        self.write_word_list_item(level + 2, w);
                    }
                }
                self.key_block(level, "body");
                for c in body {
                    self.write_statement(level + 2, c);
                }
            }
            CompoundCommand::BashCoproc { name, body, span } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "BashCoproc");
                if let Some(n) = name {
                    self.key_value(level, "name", n);
                }
                self.key_block(level, "body");
                self.write_expression(level + 2, body);
            }
            CompoundCommand::BashArithmeticFor {
                init,
                condition,
                update,
                body,
                span,
            } => {
                self.key_source(level, *span);
                self.key_value(level, "type", "BashArithmeticFor");
                if let Some(init_expr) = init {
                    self.key_block(level, "init");
                    self.write_arith_expr(level + 2, init_expr);
                }
                if let Some(cond_expr) = condition {
                    self.key_block(level, "condition");
                    self.write_arith_expr(level + 2, cond_expr);
                }
                if let Some(update_expr) = update {
                    self.key_block(level, "update");
                    self.write_arith_expr(level + 2, update_expr);
                }
                self.key_block(level, "body");
                for c in body {
                    self.write_statement(level + 2, c);
                }
            }
        }
    }

    fn write_case_arm(&mut self, level: usize, arm: &CaseArm) {
        self.indent(level);
        let src = self.source(arm.span);
        let _ = writeln!(self.buf, "- source: {}", src);
        self.key_block(level + 2, "patterns");
        for p in &arm.patterns {
            self.write_word_list_item(level + 4, p);
        }
        if !arm.body.is_empty() {
            self.key_block(level + 2, "body");
            for c in &arm.body {
                self.write_statement(level + 4, c);
            }
        }
    }

    fn write_elif(&mut self, level: usize, elif: &ElifClause) {
        self.indent(level);
        let src = self.source(elif.span);
        let _ = writeln!(self.buf, "- source: {}", src);
        self.key_block(level + 2, "condition");
        for c in &elif.condition {
            self.write_statement(level + 4, c);
        }
        self.key_block(level + 2, "body");
        for c in &elif.body {
            self.write_statement(level + 4, c);
        }
    }

    fn write_redirect(&mut self, level: usize, r: &Redirect) {
        self.indent(level);
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
        let _ = writeln!(self.buf, "- type: {}", type_name);
        self.key_source(level + 2, r.span);
        if let Some(fd) = r.fd {
            self.key_value(level + 2, "fd", &fd.to_string());
        }
        match &r.kind {
            RedirectKind::HereDoc {
                delimiter,
                body,
                strip_tabs,
                quoted,
                ..
            } => {
                self.key_value(level + 2, "delimiter", delimiter);
                if *strip_tabs {
                    self.key_value(level + 2, "strip_tabs", "true");
                }
                if *quoted {
                    self.key_value(level + 2, "quoted", "true");
                }
                self.key_block(level + 2, "body");
                self.indent(level + 4);
                self.buf.push_str("|\n");
                for line in body.lines() {
                    self.indent(level + 4);
                    let _ = writeln!(self.buf, "{}", line);
                }
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
                self.key_block(level + 2, "target");
                self.write_word_inline(level + 4, target);
            }
        }
    }

    fn write_word_list_item(&mut self, level: usize, word: &Word) {
        self.indent(level);
        // If the word has a single Literal part, use a compact form
        if word.parts.len() == 1 {
            if let Fragment::Literal(s) = &word.parts[0] {
                let _ = writeln!(self.buf, "- {}", yaml_escape(s));
                return;
            }
        }
        let src = self.source(word.span);
        let _ = writeln!(self.buf, "- source: {}", src);
        self.key_block(level + 2, "parts");
        for part in &word.parts {
            self.write_fragment(level + 4, part);
        }
    }

    fn write_argument(&mut self, level: usize, arg: &Argument) {
        match arg {
            Argument::Word(w) => {
                self.write_word_list_item(level, w);
            }
            Argument::Atom(atom) => {
                self.write_atom(level, atom);
            }
        }
    }

    fn write_atom(&mut self, level: usize, atom: &Atom) {
        match atom {
            Atom::BashProcessSubstitution {
                direction,
                body,
                span,
            } => {
                self.indent(level);
                let _ = writeln!(self.buf, "- type: BashProcessSubstitution");
                self.key_source(level + 2, *span);
                let dir = match direction {
                    ProcessDirection::In => "<",
                    ProcessDirection::Out => ">",
                };
                self.key_value(level + 2, "direction", dir);
                if !body.is_empty() {
                    self.key_block(level + 2, "statements");
                    for s in body {
                        self.indent(level + 4);
                        match s.mode {
                            ExecutionMode::Background => {
                                self.buf.push_str("- mode: Background\n");
                                self.write_expression(level + 6, &s.expression);
                            }
                            ExecutionMode::Terminated => {
                                self.buf.push_str("- mode: Terminated\n");
                                self.write_expression(level + 6, &s.expression);
                            }
                            ExecutionMode::Sequential => {
                                self.buf.push_str("- ");
                                self.write_expression_inline(level + 4, &s.expression);
                            }
                        }
                    }
                }
            }
        }
    }

    fn write_word_inline(&mut self, level: usize, word: &Word) {
        // If the word has a single Literal part, use a compact form
        if word.parts.len() == 1 {
            if let Fragment::Literal(s) = &word.parts[0] {
                self.key_value(level, "literal", &yaml_escape(s));
                return;
            }
        }
        self.key_source(level, word.span);
        self.key_block(level, "parts");
        for part in &word.parts {
            self.write_fragment(level + 2, part);
        }
    }

    fn write_fragment(&mut self, level: usize, part: &Fragment) {
        self.indent(level);
        match part {
            Fragment::Literal(s) => {
                let _ = writeln!(self.buf, "- type: Literal");
                self.key_value(level + 2, "value", &yaml_escape(s));
            }
            Fragment::SingleQuoted(s) => {
                let _ = writeln!(self.buf, "- type: SingleQuoted");
                self.key_value(level + 2, "value", &yaml_escape(s));
            }
            Fragment::DoubleQuoted(parts) => {
                let _ = writeln!(self.buf, "- type: DoubleQuoted");
                self.key_block(level + 2, "parts");
                for p in parts {
                    self.write_fragment(level + 4, p);
                }
            }
            Fragment::Parameter(expansion) => {
                let _ = writeln!(self.buf, "- type: Parameter");
                self.write_param_expansion(level + 2, expansion);
            }
            Fragment::CommandSubstitution(stmts) => {
                let _ = writeln!(self.buf, "- type: CommandSubstitution");
                if !stmts.is_empty() {
                    // Note: source locations inside command substitutions are
                    // relative to the substitution content, not the original
                    // source. We emit expressions without source annotations
                    // to avoid misleading locations.
                    self.key_block(level + 2, "statements");
                    for s in stmts {
                        self.indent(level + 4);
                        match s.mode {
                            ExecutionMode::Background => {
                                self.buf.push_str("- mode: Background\n");
                                self.write_expression(level + 6, &s.expression);
                            }
                            ExecutionMode::Terminated => {
                                self.buf.push_str("- mode: Terminated\n");
                                self.write_expression(level + 6, &s.expression);
                            }
                            ExecutionMode::Sequential => {
                                self.buf.push_str("- ");
                                self.write_expression_inline(level + 4, &s.expression);
                            }
                        }
                    }
                }
            }
            Fragment::ArithmeticExpansion(expr) => {
                let _ = writeln!(self.buf, "- type: ArithmeticExpansion");
                self.key_block(level + 2, "expression");
                self.write_arith_expr(level + 4, expr);
            }
            Fragment::Glob(g) => {
                let g_str = match g {
                    GlobChar::Star => "Star",
                    GlobChar::Question => "Question",
                    GlobChar::BracketOpen => "BracketOpen",
                };
                let _ = writeln!(self.buf, "- type: Glob");
                self.key_value(level + 2, "value", g_str);
            }
            Fragment::TildePrefix(s) => {
                let _ = writeln!(self.buf, "- type: TildePrefix");
                if s.is_empty() {
                    self.key_value(level + 2, "value", "~");
                } else {
                    self.key_value(level + 2, "value", &format!("~{}", s));
                }
            }
            Fragment::BashAnsiCQuoted(s) => {
                let _ = writeln!(self.buf, "- type: BashAnsiCQuoted");
                self.key_value(level + 2, "value", &yaml_escape(s));
            }
            Fragment::BashLocaleQuoted(inner) => {
                let _ = writeln!(self.buf, "- type: BashLocaleQuoted");
                self.key_block(level + 2, "parts");
                for p in inner {
                    self.write_fragment(level + 4, p);
                }
            }
            Fragment::BashBraceExpansion(brace) => {
                let _ = writeln!(self.buf, "- type: BashBraceExpansion");
                match brace {
                    BraceExpansionKind::List(items) => {
                        self.key_value(level + 2, "kind", "list");
                        self.key_block(level + 2, "items");
                        for item in items {
                            if item.len() == 1 {
                                if let Fragment::Literal(s) = &item[0] {
                                    self.indent(level + 4);
                                    let _ = writeln!(self.buf, "- {}", yaml_escape(s));
                                    continue;
                                }
                            }
                            self.indent(level + 4);
                            let _ = writeln!(self.buf, "- parts:");
                            for p in item {
                                self.write_fragment(level + 8, p);
                            }
                        }
                    }
                    BraceExpansionKind::Sequence { start, end, step } => {
                        self.key_value(level + 2, "kind", "sequence");
                        self.key_value(level + 2, "start", start);
                        self.key_value(level + 2, "end", end);
                        if let Some(s) = step {
                            self.key_value(level + 2, "step", s);
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
                let _ = writeln!(self.buf, "- type: BashExtGlob");
                self.key_value(level + 2, "kind", kind_str);
                self.key_value(level + 2, "pattern", &yaml_escape(pattern));
            }
        }
    }

    fn write_test_expr(&mut self, level: usize, expr: &BashTestExpr) {
        match expr {
            BashTestExpr::Unary { op, arg } => {
                self.key_value(level, "type", "Unary");
                self.key_value(level, "op", Self::unary_test_op_str(*op));
                self.key_block(level, "arg");
                self.write_word_inline(level + 2, arg);
            }
            BashTestExpr::Binary { left, op, right } => {
                self.key_value(level, "type", "Binary");
                self.key_value(level, "op", Self::binary_test_op_str(*op));
                self.key_block(level, "left");
                self.write_word_inline(level + 2, left);
                self.key_block(level, "right");
                self.write_word_inline(level + 2, right);
            }
            BashTestExpr::And { left, right } => {
                self.key_value(level, "type", "And");
                self.key_block(level, "left");
                self.write_test_expr(level + 2, left);
                self.key_block(level, "right");
                self.write_test_expr(level + 2, right);
            }
            BashTestExpr::Or { left, right } => {
                self.key_value(level, "type", "Or");
                self.key_block(level, "left");
                self.write_test_expr(level + 2, left);
                self.key_block(level, "right");
                self.write_test_expr(level + 2, right);
            }
            BashTestExpr::Not(inner) => {
                self.key_value(level, "type", "Not");
                self.key_block(level, "expr");
                self.write_test_expr(level + 2, inner);
            }
            BashTestExpr::Group(inner) => {
                self.key_value(level, "type", "Group");
                self.key_block(level, "expr");
                self.write_test_expr(level + 2, inner);
            }
            BashTestExpr::Word(w) => {
                self.key_value(level, "type", "Word");
                self.key_block(level, "value");
                self.write_word_inline(level + 2, w);
            }
        }
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

    /// Write an `ArithExpr` node.
    ///
    /// TODO: Produce a full tree when the arithmetic parser is implemented.
    /// For now, the only variant in use is `Variable` holding a raw string.
    fn write_arith_expr(&mut self, level: usize, expr: &ArithExpr) {
        match expr {
            ArithExpr::Number(n) => {
                self.key_value(level, "type", "Number");
                self.key_value(level, "value", &n.to_string());
            }
            ArithExpr::Variable(s) => {
                self.key_value(level, "type", "Variable");
                self.key_value(level, "value", &yaml_escape(s));
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
                self.key_value(level, "type", "Binary");
                self.key_value(level, "op", op_str);
                self.key_block(level, "left");
                self.write_arith_expr(level + 2, left);
                self.key_block(level, "right");
                self.write_arith_expr(level + 2, right);
            }
            ArithExpr::UnaryPrefix { op, operand } => {
                let op_str = Self::arith_unary_op_str(*op);
                self.key_value(level, "type", "UnaryPrefix");
                self.key_value(level, "op", op_str);
                self.key_block(level, "operand");
                self.write_arith_expr(level + 2, operand);
            }
            ArithExpr::UnaryPostfix { operand, op } => {
                let op_str = Self::arith_unary_op_str(*op);
                self.key_value(level, "type", "UnaryPostfix");
                self.key_value(level, "op", op_str);
                self.key_block(level, "operand");
                self.write_arith_expr(level + 2, operand);
            }
            ArithExpr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.key_value(level, "type", "Ternary");
                self.key_block(level, "condition");
                self.write_arith_expr(level + 2, condition);
                self.key_block(level, "then");
                self.write_arith_expr(level + 2, then_expr);
                self.key_block(level, "else");
                self.write_arith_expr(level + 2, else_expr);
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
                self.key_value(level, "type", "Assignment");
                self.key_value(level, "target", target);
                self.key_value(level, "op", op_str);
                self.key_block(level, "value");
                self.write_arith_expr(level + 2, value);
            }
            ArithExpr::Group(inner) => {
                self.key_value(level, "type", "Group");
                self.key_block(level, "expr");
                self.write_arith_expr(level + 2, inner);
            }
            ArithExpr::Comma { left, right } => {
                self.key_value(level, "type", "Comma");
                self.key_block(level, "left");
                self.write_arith_expr(level + 2, left);
                self.key_block(level, "right");
                self.write_arith_expr(level + 2, right);
            }
        }
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

    fn write_param_expansion(&mut self, level: usize, exp: &ParameterExpansion) {
        match exp {
            ParameterExpansion::Simple(name) => {
                self.key_value(level, "name", &format!("${}", name));
            }
            ParameterExpansion::Complex {
                name,
                operator,
                argument,
            } => {
                self.key_value(level, "name", name);
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
                    };
                    self.key_value(level, "operator", op_str);
                }
                if let Some(arg) = argument {
                    self.key_block(level, "argument");
                    self.write_word_inline(level + 2, arg);
                }
            }
        }
    }
}

/// Escape a string for YAML output. Quotes it if it contains special chars.
fn yaml_escape(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quoting = s.contains(':')
        || s.contains('#')
        || s.contains('\'')
        || s.contains('"')
        || s.contains('\n')
        || s.contains('\\')
        || s.contains('[')
        || s.contains(']')
        || s.contains('{')
        || s.contains('}')
        || s.contains('&')
        || s.contains('*')
        || s.contains('!')
        || s.contains('|')
        || s.contains('>')
        || s.contains('%')
        || s.contains('@')
        || s.contains('`')
        || s.contains(',')
        || s.contains('?')
        || s.starts_with(' ')
        || s.ends_with(' ')
        || s.starts_with('-')
        || s == "true"
        || s == "false"
        || s == "null"
        || s == "~"
        || s.parse::<f64>().is_ok();

    if needs_quoting {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}
