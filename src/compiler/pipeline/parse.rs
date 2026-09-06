use crate::parser;
use crate::diagnostics::Diagnostic;
use super::CompilationPipeline;

impl CompilationPipeline {
    /// Parse source code using the complete program parser
    pub fn parse_source(&self, source: &str, _file_name: Option<&str>) -> Result<parser::Program, Vec<Diagnostic>> {
        match parser::parse_program(source) {
            Ok(program) => Ok(program),
            Err(parse_errors) => {
                // Convert parse errors to diagnostics using the enhanced error system
                let diagnostics: Vec<Diagnostic> = parse_errors
                    .into_iter()
                    .map(|e| e.into())
                    .collect();
                Err(diagnostics)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_source_keeps_parse_program_stmt_spans() {
        let src = "x = 1\n";
        let parsed = parser::parse_program(src).expect("parse_program");
        assert_eq!(parsed.stmt_spans.len(), 1, "fixture must parse as one statement");
        assert_ne!(
            parsed.stmt_spans[0],
            crate::types::definition::Span::new(0, 0),
            "parse_program must give a real span for this line"
        );

        let pipeline = CompilationPipeline::new();
        let program = pipeline.parse_source(src, None).expect("parse_source");
        assert_eq!(program.statements.len(), 1);
        assert_eq!(program.stmt_spans.len(), 1);
        assert_eq!(
            program.stmt_spans, parsed.stmt_spans,
            "parse_source must keep parse_program spans, not drop them"
        );
        assert_ne!(
            program.stmt_spans[0],
            crate::types::definition::Span::new(0, 0),
            "parse_source must not synthesize dummy (0,0) spans"
        );
    }
}
