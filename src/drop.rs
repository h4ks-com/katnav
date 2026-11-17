//! Files dropped on the terminal. A terminal turns a file dragged onto its
//! window into pasted text, its path quoted the way a shell reads it, so
//! we read a paste of nothing but existing paths as a file drop.

use std::path::PathBuf;

/// The files a paste names, when it names nothing else and each exists.
pub(crate) fn dropped_files(text: &str) -> Option<Vec<PathBuf>> {
    let files: Vec<PathBuf> = words(text)?.into_iter().map(path).collect();
    let all_exist = files.iter().all(|file| file.is_absolute() && file.exists());
    (!files.is_empty() && all_exist).then_some(files)
}

/// The paste split into words as a shell splits them: whitespace between
/// words, quotes and backslashes keep a space inside one. A quote left
/// open makes it no list of paths at all.
fn words(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None | Some('"'), '\\') if escapes() => {
                word.push(chars.next()?);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (_, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        words.push(word);
    }
    Some(words)
}

/// Whether a backslash escapes the next character, which it does in every
/// shell but on Windows, where it separates the folders of a path.
fn escapes() -> bool {
    !cfg!(windows)
}

/// A word as a path, decoding a `file://` URL.
fn path(word: String) -> PathBuf {
    match word.strip_prefix("file://") {
        Some(rest) => PathBuf::from(percent_decoded(
            rest.strip_prefix("localhost").unwrap_or(rest),
        )),
        None => PathBuf::from(word),
    }
}

fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let hex = bytes
            .get(at + 1..at + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        if let (b'%', Some(value)) = (byte, hex) {
            decoded.push(value);
            at += 3;
        } else {
            decoded.push(byte);
            at += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{dropped_files, words};

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, b"katnav").expect("the temp dir is writable");
        path
    }

    #[cfg(not(windows))]
    #[test]
    fn shell_quoting_keeps_spaces_inside_a_word() {
        assert_eq!(
            words(r#"/a/b\ c.png '/d e/f' "/g h" "#),
            Some(vec!["/a/b c.png".into(), "/d e/f".into(), "/g h".into()])
        );
        assert_eq!(words("/a 'open"), None);
    }

    #[test]
    fn a_paste_of_existing_files_is_a_drop() {
        let first = scratch("katnav drop one.txt");
        let second = scratch("katnav-drop-two.txt");
        let pasted = format!("'{}' {}\n", first.display(), second.display());
        assert_eq!(
            dropped_files(&pasted),
            Some(vec![first.clone(), second.clone()])
        );
        let url = format!("file://{}", first.display()).replace(' ', "%20");
        assert_eq!(dropped_files(&url), Some(vec![first.clone()]));
        std::fs::remove_file(first).expect("the file is ours");
        std::fs::remove_file(second).expect("the file is ours");
    }

    #[test]
    fn text_or_a_missing_file_stays_a_paste() {
        assert_eq!(dropped_files("hello world"), None);
        assert_eq!(dropped_files("/no/such/katnav/file.png"), None);
        assert_eq!(dropped_files("   "), None);
        assert_eq!(dropped_files("relative/path.txt"), None);
    }
}
