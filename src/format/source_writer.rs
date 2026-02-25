//! Canonical shell source reconstruction from AST.
//!
//! Produces bash-compatible source text from parsed AST nodes, following
//! bash's canonical formatting (as output by `declare -f`). Does not
//! preserve original whitespace or comments — the output is normalized.

use crate::ast::*;
use crate::exec::environment::StoredFunction;
use crate::visit::Visit;

/// Reconstructs shell source from AST nodes.
///
/// Uses the [`Visit`] trait to walk the AST, overriding every method to
/// produce output rather than calling the default `walk_*` functions.
pub struct SourceWriter {
    output: String,
    indent: usize,
}

impl SourceWriter {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    /// Format a stored function definition for `declare -f` output.
    pub fn format_function(name: &str, func: &StoredFunction) -> String {
        let mut w = Self::new();
        w.push(name);
        w.push(" ()\n");
        w.emit_compound_command(&func.body);
        for r in &func.redirects {
            w.push(" ");
            w.emit_redirect(r);
        }
        w.push("\n");
        w.output
    }

    /// Format a complete program.
    pub fn format_program(program: &Program) -> String {
        let mut w = Self::new();
        w.visit_program(program);
        w.output
    }

    // Helper methods ==============================================================================================

    fn push(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            self.push("    ");
        }
    }

    /// Emit lines as indented block (for compound command bodies).
    /// Each statement gets its own line; `;` terminators are dropped
    /// (canonical bash output uses newlines instead).
    fn emit_lines(&mut self, lines: &[Line]) {
        for line in lines {
            for stmt in line.iter() {
                self.push_indent();
                self.visit_expression(&stmt.expression);
                if stmt.mode == ExecutionMode::Background {
                    self.push(" &");
                }
                self.push("\n");
            }
        }
    }

    /// Emit lines inline (semicolon-separated, for conditions).
    fn emit_lines_inline(&mut self, lines: &[Line]) {
        let stmts: Vec<&Statement> = lines.iter().flat_map(|l| l.iter()).collect();
        for (i, stmt) in stmts.iter().enumerate() {
            self.visit_expression(&stmt.expression);
            if stmt.mode == ExecutionMode::Background {
                self.push(" &");
            }
            if i + 1 < stmts.len() {
                self.push("; ");
            }
        }
    }

    fn emit_compound_command(&mut self, compound: &CompoundCommand) {
        self.visit_compound_command(compound);
    }

    fn emit_redirect(&mut self, redirect: &Redirect) {
        if let Some(fd) = redirect.fd {
            // Only print fd if non-default
            let default_fd = match &redirect.kind {
                RedirectKind::Input(_) | RedirectKind::ReadWrite(_) | RedirectKind::DupInput(_) => 0,
                _ => 1,
            };
            if fd != default_fd {
                self.push(&fd.to_string());
            }
        }
        match &redirect.kind {
            RedirectKind::Input(w) => {
                self.push("< ");
                self.visit_word(w);
            }
            RedirectKind::Output(w) => {
                self.push("> ");
                self.visit_word(w);
            }
            RedirectKind::Append(w) => {
                self.push(">> ");
                self.visit_word(w);
            }
            RedirectKind::Clobber(w) => {
                self.push(">| ");
                self.visit_word(w);
            }
            RedirectKind::ReadWrite(w) => {
                self.push("<> ");
                self.visit_word(w);
            }
            RedirectKind::DupInput(w) => {
                self.push("<& ");
                self.visit_word(w);
            }
            RedirectKind::DupOutput(w) => {
                self.push(">& ");
                self.visit_word(w);
            }
            RedirectKind::HereDoc {
                delimiter,
                body,
                strip_tabs,
                quoted,
            } => {
                if *strip_tabs {
                    self.push("<<- ");
                } else {
                    self.push("<< ");
                }
                if *quoted {
                    self.push("'");
                    self.push(delimiter);
                    self.push("'");
                } else {
                    self.push(delimiter);
                }
                self.push("\n");
                self.push(body);
                self.push(delimiter);
            }
            RedirectKind::BashHereString(w) => {
                self.push("<<< ");
                self.visit_word(w);
            }
            RedirectKind::BashOutputAll(w) => {
                self.push("&> ");
                self.visit_word(w);
            }
            RedirectKind::BashAppendAll(w) => {
                self.push("&>> ");
                self.visit_word(w);
            }
        }
    }

    fn emit_arith_op(&mut self, op: ArithBinaryOp) {
        self.push(match op {
            ArithBinaryOp::Add => " + ",
            ArithBinaryOp::Sub => " - ",
            ArithBinaryOp::Mul => " * ",
            ArithBinaryOp::Div => " / ",
            ArithBinaryOp::Mod => " % ",
            ArithBinaryOp::Exp => " ** ",
            ArithBinaryOp::ShiftLeft => " << ",
            ArithBinaryOp::ShiftRight => " >> ",
            ArithBinaryOp::BitAnd => " & ",
            ArithBinaryOp::BitOr => " | ",
            ArithBinaryOp::BitXor => " ^ ",
            ArithBinaryOp::LogAnd => " && ",
            ArithBinaryOp::LogOr => " || ",
            ArithBinaryOp::Eq => " == ",
            ArithBinaryOp::Ne => " != ",
            ArithBinaryOp::Lt => " < ",
            ArithBinaryOp::Le => " <= ",
            ArithBinaryOp::Gt => " > ",
            ArithBinaryOp::Ge => " >= ",
        });
    }

    fn emit_arith_unary_op(&mut self, op: ArithUnaryOp) {
        self.push(match op {
            ArithUnaryOp::Negate => "-",
            ArithUnaryOp::Plus => "+",
            ArithUnaryOp::LogNot => "!",
            ArithUnaryOp::BitNot => "~",
            ArithUnaryOp::Increment => "++",
            ArithUnaryOp::Decrement => "--",
        });
    }

    fn emit_arith_assign_op(&mut self, op: ArithAssignOp) {
        self.push(match op {
            ArithAssignOp::Assign => " = ",
            ArithAssignOp::AddAssign => " += ",
            ArithAssignOp::SubAssign => " -= ",
            ArithAssignOp::MulAssign => " *= ",
            ArithAssignOp::DivAssign => " /= ",
            ArithAssignOp::ModAssign => " %= ",
            ArithAssignOp::ShiftLeftAssign => " <<= ",
            ArithAssignOp::ShiftRightAssign => " >>= ",
            ArithAssignOp::BitAndAssign => " &= ",
            ArithAssignOp::BitOrAssign => " |= ",
            ArithAssignOp::BitXorAssign => " ^= ",
        });
    }

    fn emit_unary_test_op(&mut self, op: UnaryTestOp) {
        self.push(match op {
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
        });
    }

    fn emit_binary_test_op(&mut self, op: BinaryTestOp) {
        self.push(match op {
            BinaryTestOp::StringEquals => " == ",
            BinaryTestOp::StringNotEquals => " != ",
            BinaryTestOp::StringLessThan => " < ",
            BinaryTestOp::StringGreaterThan => " > ",
            BinaryTestOp::RegexMatch => " =~ ",
            BinaryTestOp::IntEq => " -eq ",
            BinaryTestOp::IntNe => " -ne ",
            BinaryTestOp::IntLt => " -lt ",
            BinaryTestOp::IntLe => " -le ",
            BinaryTestOp::IntGt => " -gt ",
            BinaryTestOp::IntGe => " -ge ",
            BinaryTestOp::FileNewerThan => " -nt ",
            BinaryTestOp::FileOlderThan => " -ot ",
            BinaryTestOp::FileSameDevice => " -ef ",
        });
    }

    fn emit_param_op(&mut self, op: ParamOp) {
        self.push(match op {
            ParamOp::Default => ":-",
            ParamOp::DefaultAssign => ":=",
            ParamOp::Error => ":?",
            ParamOp::Alternative => ":+",
            ParamOp::Length => unreachable!("Length handled separately"),
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
        });
    }
}

// Visit trait implementation ==========================================================================================

impl<'ast> Visit<'ast> for SourceWriter {
    fn visit_program(&mut self, program: &'ast Program) {
        self.emit_lines(&program.lines);
    }

    fn visit_statement(&mut self, stmt: &'ast Statement) {
        self.visit_expression(&stmt.expression);
        match stmt.mode {
            ExecutionMode::Sequential => {}
            ExecutionMode::Terminated => self.push(";"),
            ExecutionMode::Background => self.push(" &"),
        }
    }

    fn visit_expression(&mut self, expr: &'ast Expression) {
        match expr {
            Expression::Command(cmd) => self.visit_command(cmd),
            Expression::Compound { body, redirects } => {
                self.visit_compound_command(body);
                for r in redirects {
                    self.push(" ");
                    self.emit_redirect(r);
                }
            }
            Expression::FunctionDef(fndef) => self.visit_function_def(fndef),
            Expression::And { left, right } => {
                self.visit_expression(left);
                self.push(" && ");
                self.visit_expression(right);
            }
            Expression::Or { left, right } => {
                self.visit_expression(left);
                self.push(" || ");
                self.visit_expression(right);
            }
            Expression::Pipe { left, right, stderr } => {
                self.visit_expression(left);
                if *stderr {
                    self.push(" |& ");
                } else {
                    self.push(" | ");
                }
                self.visit_expression(right);
            }
            Expression::Not(inner) => {
                self.push("! ");
                self.visit_expression(inner);
            }
        }
    }

    fn visit_command(&mut self, cmd: &'ast Command) {
        let mut first = true;
        for assignment in &cmd.assignments {
            if !first {
                self.push(" ");
            }
            self.visit_assignment(assignment);
            first = false;
        }
        for arg in &cmd.arguments {
            if !first {
                self.push(" ");
            }
            self.visit_argument(arg);
            first = false;
        }
        for r in &cmd.redirects {
            self.push(" ");
            self.emit_redirect(r);
        }
    }

    fn visit_compound_command(&mut self, compound: &'ast CompoundCommand) {
        match compound {
            CompoundCommand::BraceGroup { body, .. } => {
                self.push("{\n");
                self.indent += 1;
                self.emit_lines(body);
                self.indent -= 1;
                self.push_indent();
                self.push("}");
            }
            CompoundCommand::Subshell { body, .. } => {
                self.push("(\n");
                self.indent += 1;
                self.emit_lines(body);
                self.indent -= 1;
                self.push_indent();
                self.push(")");
            }
            CompoundCommand::IfClause {
                condition,
                then_body,
                elifs,
                else_body,
                ..
            } => {
                self.push("if ");
                self.emit_lines_inline(condition);
                self.push("; then\n");
                self.indent += 1;
                self.emit_lines(then_body);
                self.indent -= 1;
                for elif in elifs {
                    self.visit_elif_clause(elif);
                }
                if let Some(else_lines) = else_body {
                    self.push_indent();
                    self.push("else\n");
                    self.indent += 1;
                    self.emit_lines(else_lines);
                    self.indent -= 1;
                }
                self.push_indent();
                self.push("fi");
            }
            CompoundCommand::WhileClause { condition, body, .. } => {
                self.push("while ");
                self.emit_lines_inline(condition);
                self.push("; do\n");
                self.indent += 1;
                self.emit_lines(body);
                self.indent -= 1;
                self.push_indent();
                self.push("done");
            }
            CompoundCommand::UntilClause { condition, body, .. } => {
                self.push("until ");
                self.emit_lines_inline(condition);
                self.push("; do\n");
                self.indent += 1;
                self.emit_lines(body);
                self.indent -= 1;
                self.push_indent();
                self.push("done");
            }
            CompoundCommand::ForClause {
                variable, words, body, ..
            } => {
                self.push("for ");
                self.push(variable);
                if let Some(word_list) = words {
                    self.push(" in");
                    for w in word_list {
                        self.push(" ");
                        self.visit_word(w);
                    }
                }
                self.push("; do\n");
                self.indent += 1;
                self.emit_lines(body);
                self.indent -= 1;
                self.push_indent();
                self.push("done");
            }
            CompoundCommand::CaseClause { word, arms, .. } => {
                self.push("case ");
                self.visit_word(word);
                self.push(" in\n");
                self.indent += 1;
                for arm in arms {
                    self.visit_case_arm(arm);
                }
                self.indent -= 1;
                self.push_indent();
                self.push("esac");
            }
            CompoundCommand::BashDoubleBracket { expression, .. } => {
                self.push("[[ ");
                self.visit_bash_test_expr(expression);
                self.push(" ]]");
            }
            CompoundCommand::BashArithmeticCommand { expression, .. } => {
                self.push("(( ");
                self.visit_arith_expr(expression);
                self.push(" ))");
            }
            CompoundCommand::BashSelectClause {
                variable, words, body, ..
            } => {
                self.push("select ");
                self.push(variable);
                if let Some(word_list) = words {
                    self.push(" in");
                    for w in word_list {
                        self.push(" ");
                        self.visit_word(w);
                    }
                }
                self.push("; do\n");
                self.indent += 1;
                self.emit_lines(body);
                self.indent -= 1;
                self.push_indent();
                self.push("done");
            }
            CompoundCommand::BashCoproc { name, body, .. } => {
                self.push("coproc ");
                if let Some(n) = name {
                    self.push(n);
                    self.push(" ");
                }
                self.visit_expression(body);
            }
            CompoundCommand::BashArithmeticFor {
                init,
                condition,
                update,
                body,
                ..
            } => {
                self.push("for (( ");
                if let Some(i) = init {
                    self.visit_arith_expr(i);
                }
                self.push("; ");
                if let Some(c) = condition {
                    self.visit_arith_expr(c);
                }
                self.push("; ");
                if let Some(u) = update {
                    self.visit_arith_expr(u);
                }
                self.push(" )); do\n");
                self.indent += 1;
                self.emit_lines(body);
                self.indent -= 1;
                self.push_indent();
                self.push("done");
            }
        }
    }

    fn visit_function_def(&mut self, fndef: &'ast FunctionDef) {
        self.push(&fndef.name);
        self.push(" ()\n");
        self.push_indent();
        self.visit_compound_command(&fndef.body);
        for r in &fndef.redirects {
            self.push(" ");
            self.emit_redirect(r);
        }
    }

    fn visit_case_arm(&mut self, arm: &'ast CaseArm) {
        self.push_indent();
        for (i, pattern) in arm.patterns.iter().enumerate() {
            if i > 0 {
                self.push(" | ");
            }
            self.visit_word(pattern);
        }
        self.push(")\n");
        self.indent += 1;
        self.emit_lines(&arm.body);
        self.indent -= 1;
        self.push_indent();
        match arm.terminator {
            Some(CaseTerminator::Break) | None => self.push(";;\n"),
            Some(CaseTerminator::BashFallThrough) => self.push(";;&\n"),
            Some(CaseTerminator::BashContinue) => self.push(";&\n"),
        }
    }

    fn visit_elif_clause(&mut self, elif: &'ast ElifClause) {
        self.push_indent();
        self.push("elif ");
        self.emit_lines_inline(&elif.condition);
        self.push("; then\n");
        self.indent += 1;
        self.emit_lines(&elif.body);
        self.indent -= 1;
    }

    fn visit_assignment(&mut self, assignment: &'ast Assignment) {
        self.push(&assignment.name);
        if let Some(ref idx) = assignment.index {
            self.push("[");
            self.push(idx);
            self.push("]");
        }
        self.push("=");
        match &assignment.value {
            AssignmentValue::Scalar(w) => self.visit_word(w),
            AssignmentValue::BashArray(elems) => {
                self.push("(");
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        self.push(" ");
                    }
                    match elem {
                        ArrayElement::Plain(w) => self.visit_word(w),
                        ArrayElement::Subscripted { index, value } => {
                            self.push("[");
                            self.push(index);
                            self.push("]=");
                            self.visit_word(value);
                        }
                    }
                }
                self.push(")");
            }
        }
    }

    fn visit_argument(&mut self, argument: &'ast Argument) {
        match argument {
            Argument::Word(w) => self.visit_word(w),
            Argument::Atom(a) => self.visit_atom(a),
        }
    }

    fn visit_word(&mut self, word: &'ast Word) {
        for fragment in &word.parts {
            self.visit_fragment(fragment);
        }
    }

    fn visit_atom(&mut self, atom: &'ast Atom) {
        match atom {
            Atom::BashProcessSubstitution { direction, body, .. } => {
                match direction {
                    ProcessDirection::In => self.push("<("),
                    ProcessDirection::Out => self.push(">("),
                }
                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        self.push("; ");
                    }
                    self.visit_statement(stmt);
                }
                self.push(")");
            }
        }
    }

    fn visit_redirect(&mut self, redirect: &'ast Redirect) {
        self.emit_redirect(redirect);
    }

    fn visit_fragment(&mut self, fragment: &'ast Fragment) {
        match fragment {
            Fragment::Literal(s) => self.push(s),
            Fragment::SingleQuoted(s) => {
                self.push("'");
                self.push(s);
                self.push("'");
            }
            Fragment::DoubleQuoted(parts) => {
                self.push("\"");
                for part in parts {
                    self.visit_fragment(part);
                }
                self.push("\"");
            }
            Fragment::Parameter(expansion) => self.visit_parameter_expansion(expansion),
            Fragment::CommandSubstitution(stmts) => {
                self.push("$(");
                for (i, stmt) in stmts.iter().enumerate() {
                    if i > 0 {
                        self.push("; ");
                    }
                    self.visit_statement(stmt);
                }
                self.push(")");
            }
            Fragment::ArithmeticExpansion(expr) => {
                self.push("$((");
                self.visit_arith_expr(expr);
                self.push("))");
            }
            Fragment::Glob(g) => match g {
                GlobChar::Star => self.push("*"),
                GlobChar::Question => self.push("?"),
                GlobChar::BracketOpen => self.push("["),
            },
            Fragment::TildePrefix(user) => {
                self.push("~");
                self.push(user);
            }
            Fragment::BashAnsiCQuoted(s) => {
                self.push("$'");
                self.push(s);
                self.push("'");
            }
            Fragment::BashLocaleQuoted { raw, .. } => {
                self.push("$\"");
                self.push(raw);
                self.push("\"");
            }
            Fragment::BashExtGlob { kind, pattern } => {
                self.push(match kind {
                    ExtGlobKind::ZeroOrOne => "?(",
                    ExtGlobKind::ZeroOrMore => "*(",
                    ExtGlobKind::OneOrMore => "+(",
                    ExtGlobKind::ExactlyOne => "@(",
                    ExtGlobKind::Not => "!(",
                });
                self.push(pattern);
                self.push(")");
            }
            Fragment::BashBraceExpansion(kind) => match kind {
                BraceExpansionKind::List(items) => {
                    self.push("{");
                    for (i, fragments) in items.iter().enumerate() {
                        if i > 0 {
                            self.push(",");
                        }
                        for f in fragments {
                            self.visit_fragment(f);
                        }
                    }
                    self.push("}");
                }
                BraceExpansionKind::Sequence { start, end, step } => {
                    self.push("{");
                    self.push(start);
                    self.push("..");
                    self.push(end);
                    if let Some(s) = step {
                        self.push("..");
                        self.push(s);
                    }
                    self.push("}");
                }
            },
        }
    }

    fn visit_parameter_expansion(&mut self, expansion: &'ast ParameterExpansion) {
        match expansion {
            ParameterExpansion::Simple(name) => {
                self.push("$");
                self.push(name);
            }
            ParameterExpansion::Complex {
                name,
                indirect,
                operator,
                argument,
            } => {
                self.push("${");
                if *indirect {
                    self.push("!");
                }
                if let Some(ParamOp::Length) = operator {
                    self.push("#");
                    self.push(name);
                } else {
                    self.push(name);
                    if let Some(op) = operator {
                        self.emit_param_op(*op);
                        if let Some(arg) = argument {
                            self.visit_word(arg);
                        }
                    }
                }
                self.push("}");
            }
        }
    }

    fn visit_arith_expr(&mut self, expr: &'ast ArithExpr) {
        match expr {
            ArithExpr::Number(n) => self.push(&n.to_string()),
            ArithExpr::Variable(name) => self.push(name),
            ArithExpr::Binary { left, op, right } => {
                self.visit_arith_expr(left);
                self.emit_arith_op(*op);
                self.visit_arith_expr(right);
            }
            ArithExpr::UnaryPrefix { op, operand } => {
                self.emit_arith_unary_op(*op);
                self.visit_arith_expr(operand);
            }
            ArithExpr::UnaryPostfix { operand, op } => {
                self.visit_arith_expr(operand);
                self.emit_arith_unary_op(*op);
            }
            ArithExpr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.visit_arith_expr(condition);
                self.push(" ? ");
                self.visit_arith_expr(then_expr);
                self.push(" : ");
                self.visit_arith_expr(else_expr);
            }
            ArithExpr::Assignment { target, op, value } => {
                self.push(target);
                self.emit_arith_assign_op(*op);
                self.visit_arith_expr(value);
            }
            ArithExpr::Group(inner) => {
                self.push("(");
                self.visit_arith_expr(inner);
                self.push(")");
            }
            ArithExpr::Comma { left, right } => {
                self.visit_arith_expr(left);
                self.push(", ");
                self.visit_arith_expr(right);
            }
        }
    }

    fn visit_bash_test_expr(&mut self, expr: &'ast BashTestExpr) {
        match expr {
            BashTestExpr::Unary { op, arg } => {
                self.emit_unary_test_op(*op);
                self.push(" ");
                self.visit_word(arg);
            }
            BashTestExpr::Binary { left, op, right } => {
                self.visit_word(left);
                self.emit_binary_test_op(*op);
                self.visit_word(right);
            }
            BashTestExpr::And { left, right } => {
                self.visit_bash_test_expr(left);
                self.push(" && ");
                self.visit_bash_test_expr(right);
            }
            BashTestExpr::Or { left, right } => {
                self.visit_bash_test_expr(left);
                self.push(" || ");
                self.visit_bash_test_expr(right);
            }
            BashTestExpr::Not(inner) => {
                self.push("! ");
                self.visit_bash_test_expr(inner);
            }
            BashTestExpr::Group(inner) => {
                self.push("( ");
                self.visit_bash_test_expr(inner);
                self.push(" )");
            }
            BashTestExpr::Word(w) => self.visit_word(w),
        }
    }
}

#[cfg(test)]
#[path = "source_writer_tests.rs"]
mod tests;
