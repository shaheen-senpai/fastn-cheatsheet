pub async fn process<'a>(
    value: ftd::ast::VariableValue,
    kind: ftd::interpreter2::Kind,
    doc: &ftd::interpreter2::TDoc<'a>,
    config: &fpm::Config,
) -> ftd::interpreter2::Result<ftd::interpreter2::Value> {
    processor_(value, kind, doc, config).await
}

pub async fn processor_<'a>(
    value: ftd::ast::VariableValue,
    kind: ftd::interpreter2::Kind,
    doc: &ftd::interpreter2::TDoc<'a>,
    config: &fpm::Config,
) -> ftd::interpreter2::Result<ftd::interpreter2::Value> {
    let (headers, body) = match value.get_record(doc.name) {
        Ok(val) => (val.2.to_owned(), val.3.to_owned()),
        Err(e) => return Err(e.into()),
    };

    let sqlite_database =
        match headers.get_optional_string_by_key("db", doc.name, value.line_number())? {
            Some(k) => k,
            None => {
                return ftd::interpreter2::utils::e2(
                    "`db` is not specified".to_string(),
                    doc.name,
                    value.line_number(),
                )
            }
        };
    let mut sqlite_database_path = camino::Utf8PathBuf::new().join(sqlite_database.as_str());
    if !sqlite_database_path.exists() {
        if !config.root.join(sqlite_database_path.as_path()).exists() {
            return ftd::interpreter2::utils::e2(
                "`db` does not exists for package-query processor".to_string(),
                doc.name,
                value.line_number(),
            );
        }
        sqlite_database_path = config.root.join(sqlite_database_path.as_path());
    }

    dbg!(&sqlite_database_path);

    let query = match &body {
        Some(b) => &b.value,
        None => {
            return ftd::interpreter2::utils::e2(
                "$processor$: `package-query` query is not specified in the processor body"
                    .to_string(),
                doc.name,
                value.line_number(),
            )
        }
    };

    dbg!(&query);
    let result = execute_query(&sqlite_database_path, query, doc.name, value.line_number()).await?;
    dbg!(&result);
    doc.from_json(&result, &kind, value.line_number())
}

async fn execute_query(
    database_path: &camino::Utf8Path,
    query: &str,
    doc_name: &str,
    line_number: usize,
) -> ftd::interpreter2::Result<serde_json::Value> {
    let conn = match rusqlite::Connection::open_with_flags(
        database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(conn) => conn,
        Err(e) => {
            return ftd::interpreter2::utils::e2(
                format!("Failed to open `{}`: {:?}", database_path, e),
                doc_name,
                line_number,
            );
        }
    };

    let mut stmt = match conn.prepare(query) {
        Ok(v) => v,
        Err(e) => {
            return ftd::interpreter2::utils::e2(
                format!("Failed to prepare query: {:?}", e),
                doc_name,
                line_number,
            )
        }
    };

    let count = stmt.column_count();

    let mut rows = match stmt.query([]) {
        Ok(v) => v,
        Err(e) => {
            return ftd::interpreter2::utils::e2(
                format!("Failed to prepare query: {:?}", e),
                doc_name,
                line_number,
            )
        }
    };

    let mut result: Vec<Vec<serde_json::Value>> = vec![];
    loop {
        match rows.next() {
            Ok(None) => break,
            Ok(Some(r)) => {
                result.push(row_to_json(r, count, doc_name, line_number)?);
            }
            Err(e) => {
                return ftd::interpreter2::utils::e2(
                    format!("Failed to execute query: {:?}", e),
                    doc_name,
                    line_number,
                )
            }
        }
    }
    let v: serde_json::Value = serde_json::to_value(&result)?;
    Ok(v)
}

fn row_to_json(
    r: &rusqlite::Row,
    count: usize,
    doc_name: &str,
    line_number: usize,
) -> ftd::interpreter2::Result<Vec<serde_json::Value>> {
    let mut row: Vec<serde_json::Value> = vec![];
    for i in 0..count {
        match r.get::<usize, rusqlite::types::Value>(i) {
            Ok(rusqlite::types::Value::Null) => row.push(serde_json::Value::Null),
            Ok(rusqlite::types::Value::Integer(i)) => row.push(serde_json::Value::Number(i.into())),
            Ok(rusqlite::types::Value::Real(i)) => row.push(serde_json::Value::Number(
                serde_json::Number::from_f64(i).unwrap(),
            )),
            Ok(rusqlite::types::Value::Text(i)) => row.push(serde_json::Value::String(i)),
            Ok(rusqlite::types::Value::Blob(_)) => {
                return ftd::interpreter2::utils::e2(
                    format!("Query returned blob for column: {}", i),
                    doc_name,
                    line_number,
                );
            }
            Err(e) => {
                return ftd::interpreter2::utils::e2(
                    format!("Failed to read response: {:?}", e),
                    doc_name,
                    line_number,
                );
            }
        }
    }
    Ok(row)
}
