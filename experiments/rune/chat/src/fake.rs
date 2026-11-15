//! Deterministic fake data: 500 tasks and a ~4,000 word markdown reply.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    NeedsYou,
    Running,
    Queued,
    Done,
    Failed,
}

pub const GROUPS: [(Status, &str); 5] = [
    (Status::NeedsYou, "Needs you"),
    (Status::Running, "Running"),
    (Status::Queued, "Queued"),
    (Status::Done, "Completed"),
    (Status::Failed, "Failed"),
];

pub struct Task {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub meta: String,
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

const VERBS: [&str; 10] = ["Fix", "Add", "Refactor", "Investigate", "Port", "Document", "Speed up", "Remove", "Rename", "Audit"];
const NOUNS: [&str; 14] = [
    "the scheduler tick", "worktree parking", "MCP connection auth", "the Linear watch", "snapshot writes", "the phone digest",
    "checkpoint undo", "goal judge", "the budget gate", "duplicate filing", "prompt goldens", "sidebar status glyphs",
    "daemon idle CPU", "the self-update clone",
];
const METAS: [&str; 6] = ["2m", "14m", "1h", "3h", "yesterday", "4d"];

pub fn tasks(n: usize) -> Vec<Task> {
    let mut r = Lcg(7);
    (0..n)
        .map(|i| {
            let status = match r.next() % 20 {
                0..=2 => Status::NeedsYou,
                3..=6 => Status::Running,
                7..=10 => Status::Queued,
                11..=17 => Status::Done,
                _ => Status::Failed,
            };
            let cost = (r.next() % 400) as f32 / 100.0;
            Task {
                id: format!("NEKO-{}", 3000 + i),
                title: format!("{} {}", r.pick(&VERBS), r.pick(&NOUNS)),
                status,
                meta: format!("{} · ${:.2}", r.pick(&METAS), cost),
            }
        })
        .collect()
}

const WORDS: &[&str] = &[
    "the", "daemon", "writes", "a", "row", "per", "task", "and", "reads", "it", "back", "when", "a", "client", "asks", "for",
    "changes", "so", "the", "snapshot", "stays", "small", "while", "agents", "keep", "working", "in", "their", "own",
    "worktrees", "without", "touching", "the", "main", "checkout", "because", "every", "run", "ends", "with", "evidence",
    "that", "the", "judge", "can", "check", "against", "what", "was", "promised", "and", "nothing", "is", "merged", "until",
    "someone", "has", "approved", "it", "or", "the", "default", "applies", "after", "a", "timeout",
];

fn paragraph(r: &mut Lcg, words: usize) -> String {
    let mut s = String::new();
    for i in 0..words {
        let w = WORDS[(r.next() % WORDS.len() as u64) as usize];
        if i == 0 {
            let mut c = w.chars();
            s.extend(c.next().map(|f| f.to_ascii_uppercase()));
            s.push_str(c.as_str());
        } else if i % 17 == 16 {
            s.push_str("**");
            s.push_str(w);
            s.push_str("**");
        } else if i % 23 == 22 {
            s.push('`');
            s.push_str(w);
            s.push('`');
        } else {
            s.push_str(w);
        }
        s.push(if i + 1 == words { '.' } else if i % 13 == 12 { ',' } else { ' ' });
        if i % 13 == 12 && i + 1 != words {
            s.push(' ');
        }
    }
    s
}

const CODE: [(&str, &str); 4] = [
    ("rust", "pub fn load(db: &Db) -> Snapshot {\n    let mut snap = Snapshot::default();\n    for row in db.tasks() {\n        snap.tasks.push(row.decode());\n    }\n    snap.heartbeat_ms = 0; // never persisted\n    snap\n}\n"),
    ("ts", "export async function watch(digest: string) {\n  const res = await request({ WorkbenchIfChanged: { digest, wait_ms: 20000 } });\n  if (res.WorkbenchUnchanged) return;\n  accept(res);\n}\n"),
    ("sh", "cargo build -p neko-daemon --bin web_harness\nNEKO_DATA_DIR=/tmp/neko-data ./target/debug/web_harness\ncurl -s localhost:4748/api/events | head\n"),
    ("json", "{\n  \"id\": \"NEKO-3004\",\n  \"status\": \"Building\",\n  \"events\": [{ \"role\": \"agent\", \"message\": \"ran tests\" }]\n}\n"),
];

/// About `target_words` words of markdown: headings, paragraphs, lists, code fences and tables.
pub fn reply(target_words: usize) -> String {
    let mut r = Lcg(42);
    let mut out = String::from("# Plan for the migration\n\nHere is the full plan, step by step, with what to run and what to expect.\n\n");
    let mut words = 20;
    let mut section = 1;
    while words < target_words {
        out.push_str(&format!("## Step {section}: {}\n\n", paragraph(&mut r, 4).trim_end_matches('.')));
        for _ in 0..2 {
            let n = 60 + (r.next() % 30) as usize;
            out.push_str(&paragraph(&mut r, n));
            out.push_str("\n\n");
            words += n;
        }
        out.push_str(&format!("### Checklist {section}\n\n"));
        for _ in 0..4 {
            let n = 8 + (r.next() % 8) as usize;
            out.push_str(&format!("- {}\n", paragraph(&mut r, n)));
            words += n;
        }
        out.push('\n');
        if section % 3 == 0 {
            let (lang, code) = CODE[(section / 3) % CODE.len()];
            out.push_str(&format!("```{lang}\n{code}```\n\n"));
            words += code.split_whitespace().count();
        }
        if section % 4 == 0 {
            out.push_str("| Task | Status | Cost | Notes |\n| --- | :---: | ---: | --- |\n");
            for i in 0..5 {
                out.push_str(&format!("| NEKO-{} | {} | ${:.2} | {} |\n", 3100 + section * 5 + i, ["done", "running", "queued"][i % 3], (r.next() % 300) as f32 / 100.0, paragraph(&mut r, 5)));
                words += 9;
            }
            out.push('\n');
        }
        section += 1;
    }
    out
}

/// Token end offsets: a token is a word with the whitespace after it, so a code fence or a table
/// streams a word at a time.
pub fn token_ends(s: &str) -> Vec<usize> {
    let b = s.as_bytes();
    let mut ends = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        ends.push(i);
    }
    ends
}
