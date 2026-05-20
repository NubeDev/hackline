//! Output formatting. Default human-readable tables; `--json` for
//! machine consumption. No business logic.

pub fn print_json(val: &serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(val).unwrap());
}

pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let cols = headers.len();
    let mut widths = headers.iter().map(|h| h.len()).collect::<Vec<_>>();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < cols && cell.len() > widths[i] {
                widths[i] = cell.len();
            }
        }
    }
    // Header
    for (i, h) in headers.iter().enumerate() {
        print!("{:<width$}  ", h, width = widths[i]);
    }
    println!();
    for w in &widths {
        print!("{:-<width$}  ", "", width = w);
    }
    println!();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            let w = if i < cols { widths[i] } else { cell.len() };
            print!("{:<width$}  ", cell, width = w);
        }
        println!();
    }
}
