use is_terminal::IsTerminal;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct LogLine {
    pub service: String,
    pub text: String,
    pub is_stderr: bool,
}

pub struct LogWriter {
    rx: mpsc::Receiver<LogLine>,
    max_name_len: usize,
    use_color: bool,
}

impl LogWriter {
    pub fn new(rx: mpsc::Receiver<LogLine>, max_name_len: usize) -> Self {
        Self {
            rx,
            max_name_len,
            use_color: std::io::stdout().is_terminal(),
        }
    }

    pub async fn run(mut self) {
        let mut color_map: BTreeMap<String, usize> = BTreeMap::new();
        let mut next_color = 0usize;

        let colors: &[&str] = &[
            "\x1b[36m", // Cyan
            "\x1b[33m", // Yellow
            "\x1b[32m", // Green
            "\x1b[35m", // Magenta
            "\x1b[34m", // Blue
            "\x1b[31m", // Red
        ];
        let reset = "\x1b[0m";

        while let Some(line) = self.rx.recv().await {
            let color_idx = *color_map.entry(line.service.clone()).or_insert_with(|| {
                let idx = next_color;
                next_color = (next_color + 1) % colors.len();
                idx
            });

            if self.use_color {
                println!(
                    "{}{:>width$} |{} {}",
                    colors[color_idx],
                    line.service,
                    reset,
                    line.text,
                    width = self.max_name_len,
                );
            } else {
                println!(
                    "{:>width$} | {}",
                    line.service,
                    line.text,
                    width = self.max_name_len,
                );
            }
        }
    }
}
