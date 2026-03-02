//! Human-readable error formatting with source context and caret highlighting.

use colored::Colorize;
use thaum::exec::ExecError;

use thaum::format::SourceMapper;

fn print_error_header(msg: &str) {
    if colored::control::SHOULD_COLORIZE.should_colorize() {
        eprintln!("{}{} {}", "error".red().bold(), ":".bold(), msg.bold());
    } else {
        eprintln!("error: {msg}");
    }
}

pub(super) fn print_error(error: &thaum::ParseError, source: &str, filename: &str, mapper: &SourceMapper) {
    let colorize = colored::control::SHOULD_COLORIZE.should_colorize();
    print_error_header(&error.to_string());

    // Source context (if we have a span)
    if let Some(span) = error.span() {
        let (line_num, col) = mapper.offset_to_line_col(span.start.0);
        let line_idx = line_num - 1;

        // Location arrow
        if colorize {
            eprintln!(" {} {}:{}:{}", "-->".blue().bold(), filename, line_num, col);
        } else {
            eprintln!(" --> {filename}:{line_num}:{col}");
        }

        // Extract the source line
        let source_line = source.lines().nth(line_idx).unwrap_or("");
        let gutter_width = line_num.to_string().len();

        // Empty gutter line
        if colorize {
            eprintln!("{} {}", " ".repeat(gutter_width), "|".blue().bold());
        } else {
            eprintln!("{} |", " ".repeat(gutter_width));
        }

        // Source line
        if colorize {
            eprintln!(
                "{} {} {}",
                format!("{line_num}").blue().bold(),
                "|".blue().bold(),
                source_line
            );
        } else {
            eprintln!("{line_num} | {source_line}");
        }

        // Underline
        let underline_start = col - 1;
        let underline_len = (span.end.0 - span.start.0).max(1);
        // Don't extend past end of source line
        let underline_len = underline_len.min(source_line.len().saturating_sub(underline_start));
        let underline_len = underline_len.max(1);

        let padding = " ".repeat(underline_start);
        let carets = "^".repeat(underline_len);

        if colorize {
            eprintln!(
                "{} {} {}{}",
                " ".repeat(gutter_width),
                "|".blue().bold(),
                padding,
                carets.red().bold()
            );
        } else {
            eprintln!("{} | {}{}", " ".repeat(gutter_width), padding, carets);
        }
    }
}

pub(super) fn print_exec_error(error: &ExecError) {
    print_error_header(&error.to_string());
}
