use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CodeChunk {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

pub struct RagIndex {
    pub chunks: Vec<CodeChunk>,
    pub term_frequencies: Vec<HashMap<String, usize>>,
    pub doc_frequencies: HashMap<String, usize>,
}

impl RagIndex {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            term_frequencies: Vec::new(),
            doc_frequencies: HashMap::new(),
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<CodeChunk> {
        let query_terms = tokenize(query);
        if query_terms.is_empty() || self.chunks.is_empty() {
            return Vec::new();
        }

        let num_docs = self.chunks.len() as f64;
        let mut scores = vec![0.0; self.chunks.len()];

        for term in query_terms {
            if let Some(&df) = self.doc_frequencies.get(&term) {
                let idf = (num_docs / (df as f64)).ln();
                for (doc_idx, tf_map) in self.term_frequencies.iter().enumerate() {
                    if let Some(&tf) = tf_map.get(&term) {
                        scores[doc_idx] += (tf as f64) * idf;
                    }
                }
            }
        }

        let mut doc_scores: Vec<(usize, f64)> = scores.into_iter().enumerate().collect();
        doc_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        doc_scores.into_iter()
            .filter(|&(_, score)| score > 0.0)
            .take(limit)
            .map(|(idx, _)| self.chunks[idx].clone())
            .collect()
    }
}

static INDEX: std::sync::Mutex<Option<RagIndex>> = std::sync::Mutex::new(None);

pub fn init_index(dir: &str) {
    println!("[RAG] Indexing directory: {}", dir);
    let index = index_directory(dir);
    println!("[RAG] Indexing complete. Total chunks indexed: {}", index.chunks.len());
    let mut lock = INDEX.lock().unwrap();
    *lock = Some(index);
}

pub fn search_index(query: &str, limit: usize) -> Vec<CodeChunk> {
    let lock = INDEX.lock().unwrap();
    if let Some(ref index) = *lock {
        index.search(query, limit)
    } else {
        Vec::new()
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|w| w.len() > 1 && !is_stopword(w))
        .map(|w| w.to_string())
        .collect()
}

fn is_stopword(word: &str) -> bool {
    let stopwords: HashSet<&str> = [
        "the", "a", "an", "and", "or", "but", "if", "then", "else", "for", "while", "of", "to",
        "in", "on", "at", "by", "from", "with", "this", "that", "it", "is", "was", "were", "be",
        "been", "have", "has", "had", "do", "does", "did", "as", "about", "into"
    ].iter().cloned().collect();
    stopwords.contains(word)
}

pub fn index_directory<P: AsRef<Path>>(root: P) -> RagIndex {
    let mut index = RagIndex::new();
    let mut files = Vec::new();
    
    collect_files(root.as_ref(), &mut files);

    for path in files {
        if let Ok(content) = fs::read_to_string(&path) {
            // Get relative path if possible, to keep it clean
            let relative_path = match path.strip_prefix(root.as_ref()) {
                Ok(rel) => rel.to_string_lossy().to_string(),
                Err(_) => path.to_string_lossy().to_string(),
            };
            chunk_file(&relative_path, &content, &mut index.chunks);
        }
    }

    for chunk in &index.chunks {
        let mut tf = HashMap::new();
        let terms = tokenize(&chunk.content);
        let unique_terms: HashSet<String> = terms.iter().cloned().collect();
        
        for term in terms {
            *tf.entry(term).or_insert(0) += 1;
        }
        
        for term in unique_terms {
            *index.doc_frequencies.entry(term).or_insert(0) += 1;
        }
        
        index.term_frequencies.push(tf);
    }

    index
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "node_modules" || name == ".git" || name == "target" || name == "dist" || name == "build" || name == ".svelte-kit" {
                    continue;
                }
                collect_files(&path, files);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let valid_extensions = [
                        "rs", "js", "ts", "svelte", "py", "java", "cpp", "c", "h", "html", "css", "json", "md", "toml"
                    ];
                    if valid_extensions.contains(&ext) {
                        files.push(path);
                    }
                }
            }
        }
    }
}

fn chunk_file(file_path: &str, content: &str, chunks: &mut Vec<CodeChunk>) {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return;
    }

    let chunk_size = 40;
    let overlap = 10;

    let mut start = 0;
    while start < lines.len() {
        let end = std::cmp::min(start + chunk_size, lines.len());
        let chunk_lines = &lines[start..end];
        let chunk_content = chunk_lines.join("\n");
        
        chunks.push(CodeChunk {
            file_path: file_path.to_string(),
            start_line: start + 1,
            end_line: end,
            content: chunk_content,
        });

        if end == lines.len() {
            break;
        }
        start += chunk_size - overlap;
    }
}
