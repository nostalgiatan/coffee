use crate::parser;
use crate::diagnostics::Diagnostic;
use super::CompilationPipeline;

impl CompilationPipeline {
    /// Two top-level functions with one name in the same file: the later one would silently
    /// replace the earlier. Generic templates and `c fn` declarations are not counted.
    pub(super) fn duplicate_functions(program: &parser::Program) -> Vec<Diagnostic> {
        let mut seen = std::collections::HashSet::new();
        let mut errors = Vec::new();
        for (idx, stmt) in program.statements.iter().enumerate() {
            let parser::Statement::Function(func) = stmt else { continue };
            if func.is_c || seen.insert(func.name.clone()) {
                continue;
            }
            let span = program
                .stmt_spans
                .get(idx)
                .cloned()
                .unwrap_or_else(|| crate::types::definition::Span::new(0, func.name.len()));
            let err = crate::types::errors::TypeSystemError::duplicate(
                func.name.clone(),
                span.clone(),
                span,
            );
            errors.push(err.into());
        }
        errors
    }

    /// Parse source code using the complete program parser
    pub fn parse_source(&self, source: &str, _file_name: Option<&str>) -> Result<parser::Program, Vec<Diagnostic>> {
        match parser::parse_program(source) {
            Ok(program) => {
                let duplicates = Self::duplicate_functions(&program);
                if duplicates.is_empty() {
                    Ok(program)
                } else {
                    Err(duplicates)
                }
            }
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
