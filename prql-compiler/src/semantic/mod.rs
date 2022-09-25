mod context;
mod declarations;
mod lowering;
mod reporting;
mod resolver;
mod scope;
mod transforms;
mod type_resolver;

pub use self::context::Context;
pub use self::declarations::{Declaration, Declarations};
pub use self::scope::{split_var_name, Scope};
pub use reporting::{collect_frames, label_references};

use crate::ast::frame::{Frame, FrameColumn};
use crate::ast::Stmt;
use crate::ir::Query;
use crate::PRQL_VERSION;

use anyhow::{bail, Result};
use semver::{Version, VersionReq};

/// Runs semantic analysis on the query, using current state.
///
/// Note that this removes function declarations from AST and saves them as current context.
pub fn resolve(statements: Vec<Stmt>, context: Option<Context>) -> Result<(Query, Context)> {
    let context = context.unwrap_or_else(load_std_lib);

    let (statements, context) = resolver::resolve(statements, context)?;

    // TODO: make resolve return only query and remove this clone here:
    let query = lowering::lower_ast_to_ir(statements, context.clone())?;

    if let Some(ref version) = query.def.version {
        check_query_version(version, &PRQL_VERSION)?;
    }

    Ok((query, context))
}

pub fn load_std_lib() -> Context {
    use crate::parse;
    let std_lib = include_str!("./stdlib.prql");
    let statements = parse(std_lib).unwrap();

    let (_, context) = resolver::resolve(statements, Context::default()).unwrap();
    context
}

fn check_query_version(query_version: &VersionReq, prql_version: &Version) -> Result<()> {
    if !query_version.matches(prql_version) {
        bail!("This query uses a version of PRQL that is not supported by your prql-compiler. You may want to upgrade the compiler.");
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use insta::assert_yaml_snapshot;

    use super::resolve;
    use crate::{ir::Query, parse};

    fn parse_and_resolve(query: &str) -> Result<Query> {
        let (query, _) = resolve(parse(query)?, None)?;
        Ok(query)
    }

    #[test]
    fn test_header() {
        assert_yaml_snapshot!(parse_and_resolve(r###"
        prql dialect:mssql version:"0"

        from employees
        "###).unwrap(), @r###"
        ---
        def:
          version: ^0
          dialect: MsSql
        tables: []
        main_pipeline:
          - From:
              - LocalTable: employees
              - - id: 0
                  name: ~
                  expr:
                    kind:
                      ExternRef:
                        variable: "*"
                        table: 0
                    span: ~
        "### );

        assert_yaml_snapshot!(parse_and_resolve(r###"
        prql dialect:bigquery version:"0.2"

        from employees
        "###).unwrap(), @r###"
        ---
        def:
          version: ^0.2
          dialect: BigQuery
        tables: []
        main_pipeline:
          - From:
              - LocalTable: employees
              - - id: 0
                  name: ~
                  expr:
                    kind:
                      ExternRef:
                        variable: "*"
                        table: 0
                    span: ~
        "### );

        assert!(parse_and_resolve(
            r###"
        prql dialect:bigquery version:foo
        from employees
        "###,
        )
        .is_err());

        assert!(parse_and_resolve(
            r###"
        prql dialect:bigquery version:"25"
        from employees
        "###,
        )
        .is_err());

        assert!(parse_and_resolve(
            r###"
        prql dialect:yah version:foo
        from employees
        "###,
        )
        .is_err());
    }

    #[test]
    fn check_valid_version() {
        let stmt = format!(
            r#"
        prql version:"{}"
        "#,
            env!("CARGO_PKG_VERSION_MAJOR")
        );
        assert!(parse(&stmt).is_ok());

        let stmt = format!(
            r#"
            prql version:"{}.{}"
            "#,
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR")
        );
        assert!(parse(&stmt).is_ok());

        let stmt = format!(
            r#"
            prql version:"{}.{}.{}"
            "#,
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR"),
            env!("CARGO_PKG_VERSION_PATCH"),
        );
        assert!(parse(&stmt).is_ok());
    }

    #[test]
    fn check_invalid_version() {
        let stmt = format!(
            "prql version:{}\n",
            env!("CARGO_PKG_VERSION_MAJOR").parse::<usize>().unwrap() + 1
        );
        assert!(parse(&stmt).is_err());
    }
}
