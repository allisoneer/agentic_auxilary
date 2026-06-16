#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOp {
    Add {
        path: String,
        contents: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        chunks: Vec<PatchChunk>,
    },
    Delete {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchChunk {
    pub context: Option<String>,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
    pub end_of_file: bool,
}

pub fn parse_patch(patch_text: &str) -> Result<Vec<PatchOp>, String> {
    let lines = normalize_patch_text(patch_text)
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let begin = lines
        .iter()
        .position(|line| line.trim() == "*** Begin Patch")
        .ok_or_else(|| String::from("Invalid patch format: missing *** Begin Patch marker"))?;
    let end = lines
        .iter()
        .rposition(|line| line.trim() == "*** End Patch")
        .ok_or_else(|| String::from("Invalid patch format: missing *** End Patch marker"))?;

    if begin >= end {
        return Err(String::from(
            "Invalid patch format: patch end marker must come after the begin marker",
        ));
    }

    let mut index = begin + 1;
    let mut operations = Vec::new();

    while index < end {
        let line = lines[index].trim_end();
        if line.is_empty() {
            index += 1;
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Add File:") {
            let path = path.trim();
            if path.is_empty() {
                return Err(String::from("Add File operation is missing a path"));
            }

            index += 1;
            let mut contents = Vec::new();
            while index < end && !lines[index].starts_with("*** ") {
                let body = lines[index]
                    .strip_prefix('+')
                    .ok_or_else(|| format!("Invalid Add File line: {}", lines[index]))?;
                contents.push(body.to_string());
                index += 1;
            }

            operations.push(PatchOp::Add {
                path: path.to_string(),
                contents: contents.join("\n"),
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Delete File:") {
            let path = path.trim();
            if path.is_empty() {
                return Err(String::from("Delete File operation is missing a path"));
            }

            operations.push(PatchOp::Delete {
                path: path.to_string(),
            });
            index += 1;
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Update File:") {
            let path = path.trim();
            if path.is_empty() {
                return Err(String::from("Update File operation is missing a path"));
            }

            index += 1;
            let move_to = if index < end {
                lines[index].strip_prefix("*** Move to:").map(|value| {
                    index += 1;
                    value.trim().to_string()
                })
            } else {
                None
            };

            let mut chunks = Vec::new();
            while index < end && !lines[index].starts_with("*** ") {
                let header = lines[index]
                    .strip_prefix("@@")
                    .ok_or_else(|| format!("Invalid Update File chunk header: {}", lines[index]))?;
                index += 1;

                let mut old_lines = Vec::new();
                let mut new_lines = Vec::new();
                let mut end_of_file = false;

                while index < end
                    && !lines[index].starts_with("@@")
                    && !lines[index].starts_with("*** ")
                {
                    let body = &lines[index];
                    if body == "*** End of File" {
                        end_of_file = true;
                        index += 1;
                        break;
                    }

                    match body.chars().next() {
                        Some(' ') => {
                            let value = body[1..].to_string();
                            old_lines.push(value.clone());
                            new_lines.push(value);
                        }
                        Some('-') => old_lines.push(body[1..].to_string()),
                        Some('+') => new_lines.push(body[1..].to_string()),
                        _ => return Err(format!("Invalid patch line in update chunk: {body}")),
                    }

                    index += 1;
                }

                chunks.push(PatchChunk {
                    context: optional_trimmed(header),
                    old_lines,
                    new_lines,
                    end_of_file,
                });
            }

            operations.push(PatchOp::Update {
                path: path.to_string(),
                move_to,
                chunks,
            });
            continue;
        }

        return Err(format!("Unknown patch directive: {line}"));
    }

    Ok(operations)
}

pub fn apply_chunks(original_text: &str, chunks: &[PatchChunk]) -> Result<String, String> {
    let original_had_trailing_newline = original_text.ends_with('\n');
    let mut lines = split_lines(original_text);
    let mut cursor = 0_usize;

    for chunk in chunks {
        let start = locate_chunk(&lines, chunk, cursor)?;
        let end = start + chunk.old_lines.len();
        lines.splice(start..end, chunk.new_lines.clone());
        cursor = start + chunk.new_lines.len();
    }

    let mut output = lines.join("\n");
    if original_had_trailing_newline && !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

fn locate_chunk(lines: &[String], chunk: &PatchChunk, cursor: usize) -> Result<usize, String> {
    if chunk.old_lines.is_empty() {
        if chunk.end_of_file {
            return Ok(lines.len());
        }

        if let Some(context) = &chunk.context {
            let matches = lines
                .iter()
                .enumerate()
                .skip(cursor)
                .filter_map(|(index, line)| {
                    if line == context {
                        Some(index + 1)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            return unique_match(&matches, "insert location");
        }

        return Ok(cursor.min(lines.len()));
    }

    let max = lines.len().saturating_sub(chunk.old_lines.len());
    let mut matches = Vec::new();
    for start in cursor..=max {
        if lines[start..start + chunk.old_lines.len()] == chunk.old_lines[..] {
            matches.push(start);
        }
    }
    if matches.is_empty() {
        for start in 0..=max {
            if lines[start..start + chunk.old_lines.len()] == chunk.old_lines[..] {
                matches.push(start);
            }
        }
    }

    unique_match(&matches, "update chunk")
}

fn unique_match(matches: &[usize], label: &str) -> Result<usize, String> {
    match matches {
        [index] => Ok(*index),
        [] => Err(format!("Failed to locate {label} in target file")),
        _ => Err(format!("Found multiple possible matches for {label}")),
    }
}

fn split_lines(text: &str) -> Vec<String> {
    let mut lines = text.replace("\r\n", "\n").replace('\r', "\n");
    if lines.ends_with('\n') {
        lines.pop();
    }

    if lines.is_empty() {
        Vec::new()
    } else {
        lines.split('\n').map(ToOwned::to_owned).collect()
    }
}

fn normalize_patch_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn optional_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_add_update_delete_and_move_operations() {
        let patch = r"*** Begin Patch
*** Add File: new.txt
+hello
*** Update File: old.txt
*** Move to: moved.txt
@@
-old
+new
*** Delete File: gone.txt
*** End Patch";

        let operations = parse_patch(patch).unwrap();

        assert_eq!(operations.len(), 3);
        assert!(matches!(operations[0], PatchOp::Add { .. }));
        assert!(matches!(operations[1], PatchOp::Update { .. }));
        assert!(matches!(operations[2], PatchOp::Delete { .. }));
    }

    #[test]
    fn applies_chunks_to_text() {
        let result = apply_chunks(
            "before\nold\nafter\n",
            &[PatchChunk {
                context: None,
                old_lines: vec![String::from("old")],
                new_lines: vec![String::from("new")],
                end_of_file: false,
            }],
        )
        .unwrap();

        assert_eq!(result, "before\nnew\nafter\n");
    }
}
