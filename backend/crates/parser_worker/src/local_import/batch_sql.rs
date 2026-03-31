use anyhow::Result;

pub(crate) const PERSIST_BATCH_INSERT_CHUNK_SIZE: usize = 256;

pub(crate) fn execute_batched_insert_with_suffix(
    tx: &mut impl postgres::GenericClient,
    insert_prefix: &str,
    insert_suffix: Option<&str>,
    column_patterns: &[&str],
    row_count: usize,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<()> {
    if row_count == 0 {
        return Ok(());
    }

    let statement = build_batched_insert_statement(
        insert_prefix,
        column_patterns,
        row_count,
        insert_suffix,
    );
    tx.execute(&statement, params)?;
    Ok(())
}

pub(crate) fn execute_batched_query_with_suffix(
    tx: &mut impl postgres::GenericClient,
    insert_prefix: &str,
    insert_suffix: Option<&str>,
    column_patterns: &[&str],
    row_count: usize,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<Vec<postgres::Row>> {
    if row_count == 0 {
        return Ok(Vec::new());
    }

    let statement = build_batched_insert_statement(
        insert_prefix,
        column_patterns,
        row_count,
        insert_suffix,
    );
    Ok(tx.query(&statement, params)?)
}

pub(crate) fn build_batched_insert_statement(
    insert_prefix: &str,
    column_patterns: &[&str],
    row_count: usize,
    insert_suffix: Option<&str>,
) -> String {
    let values_clause = build_batched_values_clause(row_count, column_patterns);
    match insert_suffix {
        Some(insert_suffix) => format!("{insert_prefix} VALUES {values_clause} {insert_suffix}"),
        None => format!("{insert_prefix} VALUES {values_clause}"),
    }
}

pub(crate) fn build_batched_values_clause(row_count: usize, column_patterns: &[&str]) -> String {
    let mut bind_index = 1usize;
    let mut row_sql = Vec::with_capacity(row_count);

    for _ in 0..row_count {
        let mut columns = Vec::with_capacity(column_patterns.len());
        for pattern in column_patterns {
            columns.push(pattern.replace("{}", &format!("${bind_index}")));
            bind_index += 1;
        }
        row_sql.push(format!("({})", columns.join(", ")));
    }

    row_sql.join(", ")
}
