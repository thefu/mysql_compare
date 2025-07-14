use anyhow::{anyhow, Result};
use clap::{App, Arg};
use mysql::prelude::*;
use mysql::*;
use regex::Regex;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};

#[derive(Debug)]
struct Config {
    data_source: String,
    source_schema: String,
    target_schema: String,
    diff_alters: String,
}

#[derive(Debug, Clone, PartialEq)]
struct TableDefinition {
    columns: HashMap<String, String>,
    column_positions: HashMap<String, usize>,
    primary: HashMap<String, String>,
    unique: HashMap<String, String>,
    keys: HashMap<String, String>,
    foreign: HashMap<String, String>,
    fulltext: HashMap<String, String>,
    options: HashMap<String, String>,
}

#[derive(Debug)]
struct SchemaObjects {
    objects_alters: String,
    tables: HashMap<String, (TableDefinition, TableDefinition)>,
}

impl SchemaObjects {
    fn new(target_schema: &str, source_schema: &str, data_source: &str) -> Self {
        let (source_tables, target_tables) = match data_source {
            "db" => (
                Self::get_database_tables(source_schema).unwrap(),
                Self::get_database_tables(target_schema).unwrap(),
            ),
            "file" => (
                Self::get_sql_tables(source_schema).unwrap(),
                Self::get_sql_tables(target_schema).unwrap(),
            ),
            _ => panic!("Invalid data source"),
        };

        let mut objects_alters = String::new();
        let mut tables = HashMap::new();

        // 找出差异表并生成ALTER语句
        for (table, target_def) in &target_tables {
            if let Some(source_def) = source_tables.get(table) {
                if target_def != source_def {
                    tables.insert(table.clone(), (target_def.clone(), source_def.clone()));
                }
            } else {
                objects_alters.push_str(&format!("-- {}\n", table));
                objects_alters.push_str(&format!("DROP TABLE `{}`;\n\n", table));
            }
        }

        for (table, source_def) in &source_tables {
            if !target_tables.contains_key(table) {
                objects_alters.push_str(&format!("-- {}\n", table));
                objects_alters.push_str(&format!("{};\n\n", source_def.to_sql(table)));
            }
        }

        Self {
            objects_alters,
            tables,
        }
    }

    fn get_database_tables(conn_str: &str) -> Result<HashMap<String, TableDefinition>> {
        // 解析连接字符串
        let re = Regex::new(r"([^:]*):(.*)@([^~]*)~([^~]*)").unwrap();
        let caps = re
            .captures(conn_str)
            .ok_or_else(|| anyhow!("Invalid connection string"))?;

        let opts = OptsBuilder::new()
            .user(Some(caps[1].to_string()))
            .pass(Some(caps[2].to_string()))
            .ip_or_hostname(Some(caps[3].split(':').next().unwrap()))
            .tcp_port(
                caps[3]
                    .split(':')
                    .nth(1)
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(3306),
            )
            .db_name(Some(caps[4].to_string()));

        let pool = Pool::new(opts)?;
        let mut conn = pool.get_conn()?;

        let mut tables = HashMap::new();
        let table_names: Vec<String> = conn.query("SHOW TABLES")?;

        for table_name in table_names {
            // SHOW CREATE TABLE 返回两列：表名和CREATE TABLE语句
            // 使用query_row获取整行然后提取第二列
            let row: Row = conn
                .exec_first(format!("SHOW CREATE TABLE `{}`", table_name), ())?
                .ok_or_else(|| anyhow!("Table not found: {}", table_name))?;
            
            let create_table: String = row.get(1).ok_or_else(|| anyhow!("Could not get CREATE TABLE statement"))?;

            tables.insert(table_name, Self::parse_table_definition(&create_table));
        }

        Ok(tables)
    }

    fn get_sql_tables(file_path: &str) -> io::Result<HashMap<String, TableDefinition>> {
        let content = fs::read_to_string(file_path)?;
        let re = Regex::new(r"(?i)CREATE\s*TABLE\s*`?(\w+)`?\s*\(([^;]+)\)").unwrap();

        let mut tables = HashMap::new();

        for cap in re.captures_iter(&content) {
            let table_name = cap[1].to_string();
            let table_def = Self::parse_table_definition(&cap[0]);
            tables.insert(table_name, table_def);
        }

        Ok(tables)
    }

    fn parse_table_definition(sql: &str) -> TableDefinition {
        let mut columns = HashMap::new();
        let mut column_positions = HashMap::new();
        let mut primary = HashMap::new();
        let mut unique = HashMap::new();
        let mut keys = HashMap::new();
        let mut foreign = HashMap::new();
        let mut fulltext = HashMap::new();
        let mut options = HashMap::new();

        // 解析列定义
        let column_re = Regex::new(r"`(\w+)`\s+([^,]+)").unwrap();
        for (pos, cap) in column_re.captures_iter(sql).enumerate() {
            columns.insert(cap[1].to_string(), cap[0].trim().to_string());
            column_positions.insert(cap[1].to_string(), pos + 1);
        }

        // 解析其他约束
        let constraint_re = Regex::new(
            r"(?i)(PRIMARY KEY|UNIQUE KEY|KEY|FULLTEXT KEY|CONSTRAINT)\s*(?:`(\w+)`)?\s*(\([^)]+\))"
        ).unwrap();

        for cap in constraint_re.captures_iter(sql) {
            let key_type = &cap[1];
            let key_name = cap.get(2).map_or("", |m| m.as_str());
            let definition = cap[3].to_string();

            match key_type.to_uppercase().as_str() {
                "PRIMARY KEY" => {
                    primary.insert(key_name.to_string(), definition);
                }
                "UNIQUE KEY" => {
                    unique.insert(key_name.to_string(), definition);
                }
                "KEY" => {
                    keys.insert(key_name.to_string(), definition);
                }
                "FULLTEXT KEY" => {
                    fulltext.insert(key_name.to_string(), definition);
                }
                "CONSTRAINT" => {
                    foreign.insert(key_name.to_string(), definition);
                }
                _ => {}
            }
        }

        // 解析表选项
        let options_re = Regex::new(r"(?i)ENGINE=(\w+)\s+DEFAULT\s+CHARSET=(\w+)").unwrap();
        if let Some(cap) = options_re.captures(sql) {
            options.insert("engine".to_string(), cap[1].to_string());
            options.insert("charset".to_string(), cap[2].to_string());
        }

        TableDefinition {
            columns,
            column_positions,
            primary,
            unique,
            keys,
            foreign,
            fulltext,
            options,
        }
    }
}

impl TableDefinition {
    fn to_sql(&self, table_name: &str) -> String {
        let mut sql = format!("CREATE TABLE `{}` (\n", table_name);

        // 添加列
        let mut columns: Vec<_> = self.columns.iter().collect();
        columns.sort_by_key(|(_, pos)| self.column_positions.get(*pos).unwrap_or(&0));
        for (i, (col, def)) in columns.iter().enumerate() {
            sql.push_str(&format!(
                "  {}`{}` {}",
                if i > 0 { "," } else { "" },
                col,
                def
            ));
        }

        // 添加约束
        let add_constraint =
            |sql: &mut String, constraints: &HashMap<String, String>, prefix: &str| {
                for (name, def) in constraints {
                    sql.push_str(&format!(", {} {} {}", prefix, name, def));
                }
            };

        add_constraint(&mut sql, &self.primary, "PRIMARY KEY");
        add_constraint(&mut sql, &self.unique, "UNIQUE KEY");
        add_constraint(&mut sql, &self.keys, "KEY");
        add_constraint(&mut sql, &self.foreign, "CONSTRAINT");
        add_constraint(&mut sql, &self.fulltext, "FULLTEXT KEY");

        sql.push_str("\n) ");

        // 添加表选项
        if let Some(engine) = self.options.get("engine") {
            sql.push_str(&format!("ENGINE={} ", engine));
        }
        if let Some(charset) = self.options.get("charset") {
            sql.push_str(&format!("DEFAULT CHARSET={}", charset));
        }

        sql
    }
}

fn generate_alters(schema_objects: &SchemaObjects) -> String {
    let mut alters = schema_objects.objects_alters.clone();

    for (table, (target, source)) in &schema_objects.tables {
        alters.push_str(&format!("-- {}\n", table));
        alters.push_str(&generate_table_alter(table, target, source));
        alters.push('\n');
    }

    alters
}

fn generate_table_alter(table: &str, target: &TableDefinition, source: &TableDefinition) -> String {
    let mut alter = format!("ALTER TABLE `{}`\n", table);
    let mut changes = Vec::new();

    // 比较列
    // 检查源数据库中的列 - 需要添加或修改的列
    for (col, source_def) in &source.columns {
        if let Some(target_def) = target.columns.get(col) {
            if source_def != target_def {
                changes.push(format!("MODIFY COLUMN `{}` {}", col, source_def));
            }
        } else {
            changes.push(format!("ADD COLUMN `{}` {}", col, source_def));
        }
    }
    
    // 检查目标数据库中的列 - 需要删除的列
    for (col, _) in &target.columns {
        if !source.columns.contains_key(col) {
            changes.push(format!("DROP COLUMN `{}`", col));
        }
    }

    // 比较约束
    let compare_constraints = |changes: &mut Vec<String>,
                               target: &HashMap<String, String>,
                               source: &HashMap<String, String>,
                               constraint_type: &str| {
        for (name, source_def) in source {
            if target.get(name) != Some(source_def) {
                changes.push(format!(
                    "DROP {} `{}`, ADD {} {}",
                    constraint_type, name, constraint_type, source_def
                ));
            }
        }
    };

    compare_constraints(
        &mut changes,
        &target.primary,
        &source.primary,
        "PRIMARY KEY",
    );
    compare_constraints(&mut changes, &target.unique, &source.unique, "UNIQUE INDEX");
    compare_constraints(&mut changes, &target.keys, &source.keys, "INDEX");
    compare_constraints(
        &mut changes,
        &target.foreign,
        &source.foreign,
        "FOREIGN KEY",
    );
    compare_constraints(
        &mut changes,
        &target.fulltext,
        &source.fulltext,
        "FULLTEXT INDEX",
    );

    // 比较表选项
    if target.options != source.options {
        if let (Some(engine), Some(charset)) =
            (source.options.get("engine"), source.options.get("charset"))
        {
            changes.push(format!("ENGINE={}, DEFAULT CHARSET={}", engine, charset));
        }
    }

    if changes.is_empty() {
        String::new()
    } else {
        alter.push_str(&changes.join(",\n"));
        alter.push(';');
        alter
    }
}

fn main() {
    let matches = App::new("diff_schema")
        .version("1.1.1")
        .about("Compare database schemas")
        .arg(
            Arg::with_name("data")
                .short("d")
                .long("data")
                .takes_value(true)
                .required(true)
                .help("Data source type: 'file' or 'db'"),
        )
        .arg(
            Arg::with_name("source")
                .short("s")
                .long("source")
                .takes_value(true)
                .required(true)
                .help("Source schema (file path or db connection)"),
        )
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .takes_value(true)
                .required(true)
                .help("Target schema (file path or db connection)"),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .takes_value(true)
                .required(true)
                .help("Output SQL file"),
        )
        .get_matches();

    let config = Config {
        data_source: matches.value_of("data").unwrap().to_string(),
        source_schema: matches.value_of("source").unwrap().to_string(),
        target_schema: matches.value_of("target").unwrap().to_string(),
        diff_alters: matches.value_of("output").unwrap().to_string(),
    };

    let schema_objects = SchemaObjects::new(
        &config.target_schema,
        &config.source_schema,
        &config.data_source,
    );

    let alters = generate_alters(&schema_objects);

    let mut file = File::create(&config.diff_alters).unwrap();
    writeln!(file, "-- set default character\nSET NAMES utf8;\n").unwrap();
    file.write_all(alters.as_bytes()).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_table_definition() {
        let sql = "CREATE TABLE users (
            id INT PRIMARY KEY,
            name VARCHAR(50) ENGINE=InnoDB DEFAULT CHARSET=utf8";

        let def = SchemaObjects::parse_table_definition(sql);

        assert_eq!(def.columns.get("id").unwrap(), "id INT");
        assert_eq!(def.primary.get("").unwrap(), "PRIMARY KEY");
        assert_eq!(def.options.get("engine").unwrap(), "InnoDB");
        assert_eq!(def.options.get("charset").unwrap(), "utf8");
    }
}

