# Shougun.md ①コア Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ユーザーごとの `~/Shougun.md`（Markdown）をパースし、ShogunAI の user-facing 生成の system prompt に「User Directives」として注入する基盤を作る。

**Architecture:** 純粋パーサ＋データモデル＋`render_directives` を `shogun-core/src/user_config/` に置き（DB/I-O 非依存で単体テスト可能）、fail-soft で見出しベースにパースする。ファイル監視はデスクトップ側 (`apps/desktop/src-tauri`) に `notify` ベースの watcher を新設し、変更検知→再パース→`Arc<RwLock<ShougunConfig>>` を更新（設定は on-demand で読むため内部バスイベントは不要＝YAGNI）。注入は inline の `build_prompt` を単一経路にする。設定 UI・Tauri command・CLI `config` サブコマンドを追加し、人間 UI と AI API を対称に保つ。

**Tech Stack:** Rust (workspace crates), 行ベースパーサ（Markdown ライブラリ不使用）, `dirs` crate（ホーム解決）, `notify` crate（FSEvents）, Tauri v2, React + TypeScript。

**Design spec:** `docs/superpowers/specs/2026-07-29-shougun-md-design.md`

**Canonical test commands:** `cargo test -p shogun-core` / `cargo test -p shogun-cli` / frontend は `pnpm -C apps/desktop typecheck`（型のみ）。

---

## File Structure

**新規（Rust core・純粋ロジック）**
- `crates/shogun-core/src/user_config/mod.rs` — 再エクスポート・`ShougunConfig`・`default_path()`・`load_or_create()`
- `crates/shogun-core/src/user_config/model.rs` — データ型（`Profile`/`Style`/`Charm`/`Workflow`/`ShougunConfig`/`ParseReport`/`SectionError`）
- `crates/shogun-core/src/user_config/parse.rs` — `parse_shougun(&str) -> (ShougunConfig, ParseReport)`
- `crates/shogun-core/src/user_config/directives.rs` — `render_directives(&ShougunConfig) -> String`
- `crates/shogun-core/src/user_config/sample.rs` — `sample_markdown() -> String`

**変更（Rust core）**
- `crates/shogun-core/src/lib.rs` — `pub mod user_config;`
- `crates/shogun-core/Cargo.toml` — `dirs` 依存追加
- `crates/shogun-core/src/inline.rs` — `build_prompt` / `compose_inline` に `directives: &str` を追加

**新規（デスクトップ）**
- `apps/desktop/src-tauri/src/user_config_watch.rs` — `notify` watcher ＋設定保持状態＋Tauri commands
**変更（デスクトップ）**
- `apps/desktop/src-tauri/Cargo.toml` — `notify` 依存追加
- `apps/desktop/src-tauri/src/lib.rs` — `mod user_config_watch;`・command 登録・watcher 起動
- `apps/desktop/src/App.tsx` — `PersonalizationSection` 追加・`Settings` へ挿入
- `apps/desktop/src/styles.css` — `.set__hint.is-err` 追加（無ければ）

**変更（CLI）**
- `crates/shogun-cli/src/command.rs` — `Command::Config { action }`・`ConfigAction`
- `crates/shogun-cli/src/parse.rs` — `config` サブコマンドのパース
- `crates/shogun-cli/src/main.rs` — `config` の分岐（ローカル解決、HTTP 不要）

---

## Task 1: user_config モジュール雛形とデータモデル

**Files:**
- Create: `crates/shogun-core/src/user_config/mod.rs`
- Create: `crates/shogun-core/src/user_config/model.rs`
- Modify: `crates/shogun-core/src/lib.rs`（`pub mod user_config;` を `pub mod traceview;` の並びに追加）

- [ ] **Step 1: データモデルの失敗するテストを書く**

`crates/shogun-core/src/user_config/model.rs`:
```rust
//! Shougun.md の内部データモデル（DB/I-O 非依存）。

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Profile {
    pub role: String,
    pub industry: String,
    pub tools: Vec<String>,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Style {
    pub tone: String,
    pub length: String,
    pub format_hints: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Workflow {
    pub name: String,
    pub trigger: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Charm {
    pub core_strengths: Vec<String>,
    pub persona_for_others: Vec<String>,
    pub preferred_intro_contexts: Vec<String>,
    pub ng_charm_patterns: Vec<String>,
}

/// 未知見出しは破棄せず保持する。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RawSection {
    pub heading: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ShougunConfig {
    pub profile: Profile,
    pub style: Style,
    pub principles: Vec<String>,
    pub do_not: Vec<String>,
    pub workflows: Vec<Workflow>,
    pub charm: Charm,
    /// `# Charm` のパースに失敗したら true（Charm 機能のみ無効化する）。
    pub charm_disabled: bool,
    pub unknown_sections: Vec<RawSection>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SectionError {
    pub section: String,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ParseReport {
    pub ok: bool,
    pub section_errors: Vec<SectionError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_empty() {
        let c = ShougunConfig::default();
        assert!(c.principles.is_empty());
        assert!(!c.charm_disabled);
        assert!(c.unknown_sections.is_empty());
    }
}
```

`crates/shogun-core/src/user_config/mod.rs`:
```rust
//! `Shougun.md` のパース・注入基盤（純粋ロジック）。

pub mod model;

pub use model::{
    Charm, ParseReport, Profile, RawSection, SectionError, ShougunConfig, Style, Workflow,
};
```

`crates/shogun-core/src/lib.rs`（既存 `pub mod traceview;` の近くに追加）:
```rust
pub mod user_config;
```

- [ ] **Step 2: テストが通ることを確認（コンパイル＋Default テスト）**

Run: `cargo test -p shogun-core user_config::model`
Expected: PASS（`default_config_is_empty`）

- [ ] **Step 3: Commit**

```bash
git add crates/shogun-core/src/user_config/ crates/shogun-core/src/lib.rs
git commit -m "feat(user-config): データモデルとモジュール雛形 (#41)"
```

---

## Task 2: セクション分割（見出しベース）

**Files:**
- Create: `crates/shogun-core/src/user_config/parse.rs`
- Modify: `crates/shogun-core/src/user_config/mod.rs`（`pub mod parse;` 追加）

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/user_config/parse.rs`:
```rust
//! `Shougun.md` の行ベースパーサ（fail-soft）。

use crate::user_config::model::*;

/// 見出し `# X` で本文を分割する。戻り値は (heading, start_line, body_lines)。
/// `#` 1個の ATX 見出しのみをセクション境界とする（`##` 以下は本文扱い）。
pub(crate) fn split_sections(input: &str) -> Vec<(String, usize, Vec<String>)> {
    let mut out: Vec<(String, usize, Vec<String>)> = Vec::new();
    let mut cur: Option<(String, usize, Vec<String>)> = None;
    for (i, raw) in input.lines().enumerate() {
        let line = raw.trim_end();
        if let Some(rest) = line.strip_prefix("# ") {
            if let Some(sec) = cur.take() {
                out.push(sec);
            }
            cur = Some((rest.trim().to_string(), i + 1, Vec::new()));
        } else if let Some((_, _, body)) = cur.as_mut() {
            body.push(line.to_string());
        }
    }
    if let Some(sec) = cur.take() {
        out.push(sec);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_h1_headings_only() {
        let md = "# Profile\n- Role: PM\n\n# Style\n- Tone: warm\n## Sub\ntext";
        let secs = split_sections(md);
        assert_eq!(secs.len(), 2);
        assert_eq!(secs[0].0, "Profile");
        assert_eq!(secs[0].1, 1); // line number (1-based)
        assert_eq!(secs[1].0, "Style");
        // `## Sub` は Style の本文として残る
        assert!(secs[1].2.iter().any(|l| l == "## Sub"));
    }

    #[test]
    fn text_before_first_heading_is_ignored() {
        let secs = split_sections("intro text\n# Profile\n- Role: PM");
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0].0, "Profile");
    }
}
```

`crates/shogun-core/src/user_config/mod.rs` に追記:
```rust
pub mod parse;
```

- [ ] **Step 2: テストが失敗→実装済みなので通ることを確認**

Run: `cargo test -p shogun-core user_config::parse::tests::splits`
Expected: PASS（両テスト）

- [ ] **Step 3: Commit**

```bash
git add crates/shogun-core/src/user_config/parse.rs crates/shogun-core/src/user_config/mod.rs
git commit -m "feat(user-config): 見出しベースのセクション分割 (#41)"
```

---

## Task 3: 既知セクションのパース（fail-soft）

**Files:**
- Modify: `crates/shogun-core/src/user_config/parse.rs`
- Modify: `crates/shogun-core/src/user_config/mod.rs`（`parse_shougun` 再エクスポート）

- [ ] **Step 1: 失敗するテストを書く（Issue の設定例で全フィールド検証＋Charm fail-soft）**

`parse.rs` の `tests` モジュールに追加:
```rust
    const EXAMPLE: &str = r#"# Profile
- Role: B2B SaaS のプロダクトマネージャー
- Industry: ホテル向けレベニューマネジメント
- Tools: Notion, Slack, GitHub

# Style
- Tone: 丁寧だがフレンドリー
- Length: まずは短く要点、その後に詳細

# Principles
- データドリブンであることを優先する
- ユーザー価値 > 売上 > コスト の順で判断する

# DoNot
- 根拠のない数値は出さない

# Workflows
- Name: DailyReview
  Trigger: "今日の振り返り"
  Steps:
    - 今日の出来事を 3 つに要約
    - 明日の最重要タスクを 1 つだけ決める

# Charm
- CoreStrengths:
  - 抽象度の高い概念を比喩に落とし込める
- NGCharmPatterns:
  - 過度に自己卑下するトーンは避けてほしい
"#;

    #[test]
    fn parses_example_sections() {
        let (c, report) = parse_shougun(EXAMPLE);
        assert!(report.ok, "errors: {:?}", report.section_errors);
        assert_eq!(c.profile.role, "B2B SaaS のプロダクトマネージャー");
        assert_eq!(c.profile.tools, vec!["Notion", "Slack", "GitHub"]);
        assert_eq!(c.style.tone, "丁寧だがフレンドリー");
        assert_eq!(c.principles.len(), 2);
        assert_eq!(c.do_not, vec!["根拠のない数値は出さない"]);
        assert_eq!(c.workflows.len(), 1);
        assert_eq!(c.workflows[0].name, "DailyReview");
        assert_eq!(c.workflows[0].trigger, "今日の振り返り");
        assert_eq!(c.workflows[0].steps.len(), 2);
        assert_eq!(c.charm.core_strengths.len(), 1);
        assert_eq!(c.charm.ng_charm_patterns.len(), 1);
        assert!(!c.charm_disabled);
    }

    #[test]
    fn unknown_heading_is_preserved() {
        let (c, _) = parse_shougun("# Notes\n- hello\n");
        assert_eq!(c.unknown_sections.len(), 1);
        assert_eq!(c.unknown_sections[0].heading, "Notes");
    }

    #[test]
    fn empty_input_is_ok() {
        let (c, report) = parse_shougun("");
        assert!(report.ok);
        assert_eq!(c, ShougunConfig::default());
    }
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cargo test -p shogun-core user_config::parse::tests::parses_example_sections`
Expected: FAIL（`parse_shougun` 未定義でコンパイルエラー）

- [ ] **Step 3: `parse_shougun` を実装**

`parse.rs`（`split_sections` の下、`#[cfg(test)]` の上）に追加:
```rust
/// bullet 行 `- text` を取り出す（ネストは 2 スペース以上のインデントで判定）。
fn bullets(body: &[String]) -> Vec<String> {
    body.iter()
        .filter_map(|l| l.trim().strip_prefix("- ").map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// `- Key: value` 形式から key に一致する値を返す（大文字小文字無視）。
fn field(body: &[String], key: &str) -> String {
    for l in body {
        let t = l.trim().trim_start_matches("- ").trim();
        if let Some((k, v)) = t.split_once(':') {
            if k.trim().eq_ignore_ascii_case(key) {
                return v.trim().trim_matches('"').to_string();
            }
        }
    }
    String::new()
}

fn csv(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// `- Key:` の直後にインデントされた bullet を集める（CoreStrengths など）。
fn sub_bullets(body: &[String], key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut capturing = false;
    for l in body {
        let t = l.trim();
        let key_line = t.trim_start_matches("- ").trim();
        if key_line.eq_ignore_ascii_case(&format!("{key}:"))
            || key_line.to_ascii_lowercase().starts_with(&format!("{}:", key.to_ascii_lowercase()))
        {
            capturing = true;
            continue;
        }
        // 新しい `- Key:`（コロン終わりでインデント浅い）が来たら停止
        let is_new_key = t.starts_with("- ") && key_line.ends_with(':');
        if capturing && is_new_key {
            capturing = false;
        }
        if capturing {
            if let Some(v) = t.strip_prefix("- ") {
                let v = v.trim();
                if !v.is_empty() {
                    out.push(v.to_string());
                }
            }
        }
    }
    out
}

fn parse_workflows(body: &[String]) -> Vec<Workflow> {
    let mut out = Vec::new();
    let mut cur: Option<Workflow> = None;
    let mut in_steps = false;
    for l in body {
        let t = l.trim();
        let keyed = t.trim_start_matches("- ").trim();
        if let Some(name) = keyed.strip_prefix("Name:") {
            if let Some(w) = cur.take() {
                out.push(w);
            }
            cur = Some(Workflow { name: name.trim().to_string(), ..Default::default() });
            in_steps = false;
        } else if let Some(tr) = keyed.strip_prefix("Trigger:") {
            if let Some(w) = cur.as_mut() {
                w.trigger = tr.trim().trim_matches('"').to_string();
            }
        } else if keyed.eq_ignore_ascii_case("Steps:") {
            in_steps = true;
        } else if in_steps {
            if let Some(step) = t.strip_prefix("- ") {
                if let Some(w) = cur.as_mut() {
                    w.steps.push(step.trim().to_string());
                }
            }
        }
    }
    if let Some(w) = cur.take() {
        out.push(w);
    }
    out
}

/// `Shougun.md` の内容をパースする。セクション単位でエラーを隔離する。
pub fn parse_shougun(input: &str) -> (ShougunConfig, ParseReport) {
    let mut cfg = ShougunConfig::default();
    let mut report = ParseReport { ok: true, section_errors: Vec::new() };

    for (heading, line, body) in split_sections(input) {
        match heading.as_str() {
            "Profile" => {
                cfg.profile.role = field(&body, "Role");
                cfg.profile.industry = field(&body, "Industry");
                cfg.profile.tools = csv(&field(&body, "Tools"));
                cfg.profile.topics = csv(&field(&body, "Topics"));
            }
            "Style" => {
                cfg.style.tone = field(&body, "Tone");
                cfg.style.length = field(&body, "Length");
                cfg.style.format_hints = sub_bullets(&body, "Format");
            }
            "Principles" => cfg.principles = bullets(&body),
            "DoNot" => cfg.do_not = bullets(&body),
            "Workflows" => cfg.workflows = parse_workflows(&body),
            "Charm" => {
                cfg.charm.core_strengths = sub_bullets(&body, "CoreStrengths");
                cfg.charm.persona_for_others = sub_bullets(&body, "PersonaForOthers");
                cfg.charm.preferred_intro_contexts =
                    sub_bullets(&body, "PreferredIntroductionContexts");
                cfg.charm.ng_charm_patterns = sub_bullets(&body, "NGCharmPatterns");
                // 4項目すべて空 かつ 本文はあり → フォーマット不正として Charm 無効化
                let all_empty = cfg.charm.core_strengths.is_empty()
                    && cfg.charm.persona_for_others.is_empty()
                    && cfg.charm.preferred_intro_contexts.is_empty()
                    && cfg.charm.ng_charm_patterns.is_empty();
                if all_empty && body.iter().any(|l| !l.trim().is_empty()) {
                    cfg.charm_disabled = true;
                    report.ok = false;
                    report.section_errors.push(SectionError {
                        section: "Charm".into(),
                        line,
                        message: "Charm セクションから認識可能な項目を抽出できませんでした".into(),
                    });
                }
            }
            other => cfg.unknown_sections.push(RawSection {
                heading: other.to_string(),
                body: body.join("\n"),
            }),
        }
    }
    (cfg, report)
}
```

`mod.rs` に再エクスポート追加:
```rust
pub use parse::parse_shougun;
```

- [ ] **Step 4: テストを実行して通ることを確認**

Run: `cargo test -p shogun-core user_config::parse`
Expected: PASS（`parses_example_sections` / `unknown_heading_is_preserved` / `empty_input_is_ok` を含む全テスト）

- [ ] **Step 5: Commit**

```bash
git add crates/shogun-core/src/user_config/parse.rs crates/shogun-core/src/user_config/mod.rs
git commit -m "feat(user-config): 既知セクションの fail-soft パース (#41)"
```

---

## Task 4: render_directives（system prompt 用ブロック生成）

**Files:**
- Create: `crates/shogun-core/src/user_config/directives.rs`
- Modify: `crates/shogun-core/src/user_config/mod.rs`

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/user_config/directives.rs`:
```rust
//! `ShougunConfig` を system prompt 用の "User Directives" 文字列に変換する。

use crate::user_config::model::ShougunConfig;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_config::parse::parse_shougun;

    #[test]
    fn empty_config_yields_empty_string() {
        assert_eq!(render_directives(&ShougunConfig::default()), "");
    }

    #[test]
    fn includes_principles_and_donot_and_charm() {
        let (c, _) = parse_shougun(
            "# Principles\n- データドリブン\n# DoNot\n- 数値を捏造しない\n# Charm\n- CoreStrengths:\n  - 比喩がうまい\n",
        );
        let out = render_directives(&c);
        assert!(out.contains("User Directives"));
        assert!(out.contains("データドリブン"));
        assert!(out.contains("数値を捏造しない"));
        assert!(out.contains("比喩がうまい"));
    }

    #[test]
    fn disabled_charm_is_omitted() {
        let mut c = ShougunConfig::default();
        c.charm.core_strengths = vec!["x".into()];
        c.charm_disabled = true;
        assert!(!render_directives(&c).contains('x'));
    }
}
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cargo test -p shogun-core user_config::directives`
Expected: FAIL（`render_directives` 未定義）

- [ ] **Step 3: `render_directives` を実装**

`directives.rs`（`use` の下、`#[cfg(test)]` の上）:
```rust
fn push_list(out: &mut String, label: &str, items: &[String]) {
    let items: Vec<&String> = items.iter().filter(|s| !s.trim().is_empty()).collect();
    if items.is_empty() {
        return;
    }
    out.push_str(label);
    out.push('\n');
    for it in items {
        out.push_str("- ");
        out.push_str(it.trim());
        out.push('\n');
    }
    out.push('\n');
}

/// user-facing 生成の system prompt 先頭に差し込むブロックを生成する。
/// 何も設定が無ければ空文字列を返す（呼び出し側はそのまま連結できる）。
pub fn render_directives(cfg: &ShougunConfig) -> String {
    let mut out = String::new();

    if !cfg.style.tone.trim().is_empty() {
        out.push_str(&format!("Tone: {}\n", cfg.style.tone.trim()));
    }
    if !cfg.style.length.trim().is_empty() {
        out.push_str(&format!("Length preference: {}\n", cfg.style.length.trim()));
    }
    push_list(&mut out, "Format preferences:", &cfg.style.format_hints);
    push_list(&mut out, "Principles to follow:", &cfg.principles);
    push_list(&mut out, "Never do the following:", &cfg.do_not);

    if !cfg.charm_disabled {
        push_list(&mut out, "The user's strengths (draw on these):", &cfg.charm.core_strengths);
        push_list(&mut out, "How to present the user to others:", &cfg.charm.persona_for_others);
        push_list(&mut out, "Avoid these ways of framing the user:", &cfg.charm.ng_charm_patterns);
    }

    for w in &cfg.workflows {
        if w.trigger.trim().is_empty() || w.steps.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "When the user says \"{}\", follow these steps: {}\n",
            w.trigger.trim(),
            w.steps.join(" → ")
        ));
    }

    if out.trim().is_empty() {
        return String::new();
    }
    format!("User Directives (the user has declared these preferences — honor them):\n{out}")
}
```

`mod.rs` に追加:
```rust
pub mod directives;
pub use directives::render_directives;
```

- [ ] **Step 4: テストを実行して通ることを確認**

Run: `cargo test -p shogun-core user_config::directives`
Expected: PASS（3 テスト）

- [ ] **Step 5: Commit**

```bash
git add crates/shogun-core/src/user_config/directives.rs crates/shogun-core/src/user_config/mod.rs
git commit -m "feat(user-config): render_directives の生成 (#41)"
```

---

## Task 5: サンプルテンプレート

**Files:**
- Create: `crates/shogun-core/src/user_config/sample.rs`
- Modify: `crates/shogun-core/src/user_config/mod.rs`

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/user_config/sample.rs`:
```rust
//! 初回起動時に生成するサンプル `Shougun.md`。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_config::parse::parse_shougun;

    #[test]
    fn sample_has_charm_and_warning_and_parses_clean() {
        let md = sample_markdown();
        assert!(md.contains("# Charm"));
        assert!(md.to_lowercase().contains("password") || md.contains("API"));
        let (_c, report) = parse_shougun(&md);
        assert!(report.ok, "sample must parse without errors: {:?}", report.section_errors);
    }
}
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cargo test -p shogun-core user_config::sample`
Expected: FAIL（`sample_markdown` 未定義）

- [ ] **Step 3: `sample_markdown` を実装**

`sample.rs`（`#[cfg(test)]` の上）:
```rust
/// 初回生成用のサンプル。各セクションにコメント例を含み、そのままパース可能。
pub fn sample_markdown() -> String {
    // NOTE: 生のパスワード・API キーは書かないこと。
    r#"# Profile
- Role: あなたの職種を書いてください
- Industry: 業界
- Tools: Notion, Slack, GitHub

# Style
- Tone: 丁寧だがフレンドリー
- Length: まずは短く要点、その後に詳細

# Principles
- 判断に迷ったら最初に結論を出す

# DoNot
- 根拠のない数値は出さない
- 生のパスワードや API キーはこのファイルに書かない

# Workflows
- Name: DailyReview
  Trigger: "今日の振り返り"
  Steps:
    - 今日の出来事を 3 つに要約
    - 明日の最重要タスクを 1 つ決める

# Charm
- CoreStrengths:
  - あなたの強み（例：カオスから要点を抜き出せる）
- PersonaForOthers:
  - どう紹介されたいか
- PreferredIntroductionContexts:
  - 朝のレビューで今日の強みの活かしどころを一言ほしい
- NGCharmPatterns:
  - 過度に自己卑下するトーンは避けてほしい
"#
    .to_string()
}
```

`mod.rs` に追加:
```rust
pub mod sample;
pub use sample::sample_markdown;
```

- [ ] **Step 4: テストを実行して通ることを確認**

Run: `cargo test -p shogun-core user_config::sample`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/shogun-core/src/user_config/sample.rs crates/shogun-core/src/user_config/mod.rs
git commit -m "feat(user-config): サンプルテンプレート (#41)"
```

---

## Task 6: パス解決とロード（I-O）

**Files:**
- Modify: `crates/shogun-core/Cargo.toml`（`dirs` 依存追加）
- Modify: `crates/shogun-core/src/user_config/mod.rs`

- [ ] **Step 1: 依存を追加**

`crates/shogun-core/Cargo.toml` の `[dependencies]` に追加:
```toml
dirs = "5"
```

- [ ] **Step 2: 失敗するテストを書く**

`mod.rs` の末尾に追加:
```rust
#[cfg(test)]
mod io_tests {
    use super::*;

    #[test]
    fn load_or_create_writes_sample_when_missing() {
        let dir = std::env::temp_dir().join(format!("shougun_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Shougun.md");
        let _ = std::fs::remove_file(&path);

        let (cfg, created) = load_or_create(&path).expect("load_or_create");
        assert!(created, "missing file should be created");
        assert!(path.exists());
        // 2回目は作成しない
        let (_cfg2, created2) = load_or_create(&path).expect("second load");
        assert!(!created2);
        let _ = cfg; // 使用済み扱い
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 3: テストを実行して失敗を確認**

Run: `cargo test -p shogun-core user_config::io_tests`
Expected: FAIL（`load_or_create` 未定義）

- [ ] **Step 4: `default_path` と `load_or_create` を実装**

`mod.rs`（再エクスポート群の下）に追加:
```rust
use std::path::{Path, PathBuf};

/// 既定のファイルパス（ホーム直下 `~/Shougun.md`）。
pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("Shougun.md"))
}

/// ファイルを読み、無ければサンプルを書き出す。
/// 戻り値: (パース済み設定, 新規作成したか)。
pub fn load_or_create(path: &Path) -> std::io::Result<(ShougunConfig, bool)> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        let (cfg, _report) = parse_shougun(&content);
        Ok((cfg, false))
    } else {
        let sample = sample_markdown();
        std::fs::write(path, &sample)?;
        let (cfg, _report) = parse_shougun(&sample);
        Ok((cfg, true))
    }
}

/// ファイルを読んでパース結果とレポートを返す（存在しなければ空＋ok）。
pub fn load_report(path: &Path) -> std::io::Result<(ShougunConfig, ParseReport)> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        Ok(parse_shougun(&content))
    } else {
        Ok((ShougunConfig::default(), ParseReport { ok: true, section_errors: Vec::new() }))
    }
}
```

- [ ] **Step 5: テストを実行して通ることを確認**

Run: `cargo test -p shogun-core user_config`
Expected: PASS（全 user_config テスト）

- [ ] **Step 6: Commit**

```bash
git add crates/shogun-core/Cargo.toml crates/shogun-core/src/user_config/mod.rs Cargo.lock
git commit -m "feat(user-config): パス解決とロード（load_or_create） (#41)"
```

---

## Task 7: inline build_prompt への注入

**Files:**
- Modify: `crates/shogun-core/src/inline.rs`

> `build_prompt(ctx, memory)` と `compose_inline(reader, agent, inserter, memory)` に `directives: &str` を追加する。呼び出し側は `render_directives(&cfg)` の結果を渡す。空文字なら何も差し込まない。

- [ ] **Step 1: 失敗するテストを書く**

`inline.rs` の `#[cfg(test)] mod tests` に追加（`CursorContext` の構築は既存テストの `ctx` 生成に合わせること。既存テストで使われている生成ヘルパを流用する）:
```rust
    #[test]
    fn build_prompt_includes_directives_when_present() {
        let ctx = test_ctx(); // 既存テストのコンテキスト生成ヘルパに合わせる
        let p = build_prompt(&ctx, &[], "User Directives:\n- be terse\n");
        assert!(p.contains("be terse"));
    }

    #[test]
    fn build_prompt_omits_directives_when_empty() {
        let ctx = test_ctx();
        let p = build_prompt(&ctx, &[], "");
        assert!(!p.contains("User Directives"));
    }
```

> NOTE: `test_ctx()` は既存テストにある `CursorContext` 生成方法に置き換える（`inline.rs` の既存テストを参照）。無ければ最小の `CursorContext { app: "Mail".into(), field_label: String::new(), .. }` を作る。

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cargo test -p shogun-core inline::tests::build_prompt_includes_directives`
Expected: FAIL（引数の数が合わずコンパイルエラー）

- [ ] **Step 3: シグネチャと本文を変更**

`build_prompt` を次のように変更（プリアンブル直後、memory facts の前に directives を差し込む）:
```rust
pub fn build_prompt(ctx: &CursorContext, memory: &[String], directives: &str) -> String {
    let mut p = String::new();
    p.push_str("You are writing directly in the user's active app");
    // ...（既存のアプリ/フィールド記述はそのまま）...
    p.push_str(". Continue the text at the cursor in the user's own voice. ");
    p.push_str("Output only the text to insert at the cursor — no preamble, no quotation marks, no sign-off unless the context clearly calls for one.\n");

    if !directives.trim().is_empty() {
        p.push('\n');
        p.push_str(directives.trim());
        p.push('\n');
    }

    // ...（既存の memory facts 出力はそのまま）...
    p
}
```

`compose_inline` に `directives: &str` を追加し、`build_prompt` へ渡す:
```rust
pub fn compose_inline<R, A, I>(
    reader: &R,
    agent: &A,
    inserter: &I,
    memory: &[String],
    directives: &str,
) -> InlineOutcome
where
    R: CursorReader + ?Sized,
    A: AgentClient + ?Sized,
    I: TextInserter + ?Sized,
{
    let Some(ctx) = reader.read() else {
        return InlineOutcome::NoContext;
    };
    let prompt = build_prompt(&ctx, memory, directives);
    // ...（以下既存のまま）...
}
```

- [ ] **Step 4: 既存呼び出し側をすべて更新**

Run: `grep -rn "compose_inline\|build_prompt" crates apps --include=*.rs | grep -v "fn build_prompt\|fn compose_inline"`
各呼び出しに `directives` 引数を追加する。呼び出し元が `ShougunConfig` を持てる場合は `&shogun_core::user_config::render_directives(&cfg)` を渡し、まだ配線されていない箇所は暫定的に `""` を渡す（Task 8 で実データを供給）。

- [ ] **Step 5: テストとビルドを確認**

Run: `cargo test -p shogun-core inline`
Expected: PASS（追加 2 テスト＋既存テスト）
Run: `cargo build -p shogun-core`
Expected: 成功（全呼び出し側が更新済み）

- [ ] **Step 6: Commit**

```bash
git add crates/shogun-core/src/inline.rs
git commit -m "feat(user-config): build_prompt へ directives を注入 (#41)"
```

---

## Task 8: デスクトップ watcher と設定保持状態

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`（`notify` 依存追加）
- Create: `apps/desktop/src-tauri/src/user_config_watch.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`（`mod user_config_watch;`・起動時に watcher spawn・`UserConfigState` を manage）

> 既存の `capture_source` ポーラ（`spawn_capture_poller`）と同じく、Tauri プロセス内でバックグラウンドタスクを起動する。設定は `Arc<RwLock<ShougunConfig>>` に保持し、`render_directives` で参照する。

- [ ] **Step 1: 依存を追加**

`apps/desktop/src-tauri/Cargo.toml` の `[dependencies]`:
```toml
notify = "6.1"
```

- [ ] **Step 2: watcher モジュールを作成**

`apps/desktop/src-tauri/src/user_config_watch.rs`:
```rust
//! `~/Shougun.md` を監視して再パースし、共有状態を更新する。

use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use shogun_core::user_config::{default_path, load_or_create, load_report, render_directives, ShougunConfig};

/// フロントに公開する状態。
#[derive(Clone, Default)]
pub struct UserConfigState {
    pub cfg: Arc<RwLock<ShougunConfig>>,
}

impl UserConfigState {
    /// 現在の設定から directives 文字列を得る。
    pub fn directives(&self) -> String {
        self.cfg.read().map(|c| render_directives(&c)).unwrap_or_default()
    }
}

/// 起動時に呼ぶ: 初回ロード（無ければサンプル生成）＋ファイル監視を開始する。
pub fn spawn_user_config_watch(state: UserConfigState) {
    let Some(path) = default_path() else { return };

    // 初回ロード
    if let Ok((cfg, _created)) = load_or_create(&path) {
        if let Ok(mut w) = state.cfg.write() {
            *w = cfg;
        }
    }

    // 監視（別スレッド。notify のイベントは同期コールバック）
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher: RecommendedWatcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[user-config] watcher init failed: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
            eprintln!("[user-config] watch failed: {e}");
            return;
        }
        // debounce: イベントを受けたら 500ms 待ってから再パース
        while rx.recv().is_ok() {
            std::thread::sleep(Duration::from_millis(500));
            while rx.try_recv().is_ok() {} // 溜まったイベントを流す
            if let Ok((cfg, report)) = load_report(&path) {
                if let Ok(mut w) = state.cfg.write() {
                    *w = cfg;
                }
                if !report.ok {
                    eprintln!("[user-config] parse issues: {:?}", report.section_errors);
                }
            }
        }
    });
}
```

`apps/desktop/src-tauri/src/lib.rs`:
- 冒頭のモジュール宣言群に追加: `mod user_config_watch;`
- Tauri セットアップ内（他のポーラ `spawn_capture_poller` を呼んでいる箇所付近）に追加:
```rust
let user_cfg = user_config_watch::UserConfigState::default();
user_config_watch::spawn_user_config_watch(user_cfg.clone());
app.manage(user_cfg);
```

- [ ] **Step 3: ビルド確認（watcher は統合コードのため単体テストはパーサ側で担保済み）**

Run: `cargo build -p shogun-desktop`（クレート名は `apps/desktop/src-tauri/Cargo.toml` の `[package] name` に合わせる。不明なら `cargo build` をワークスペースルートで実行）
Expected: 成功

- [ ] **Step 4: inline 呼び出しへ directives を供給**

Task 7 で `""` を渡した inline 呼び出しのうち、Tauri 側から来るものを `user_cfg.directives()` に差し替える（`tauri::State<UserConfigState>` を該当コマンド/呼び出しに追加して取得）。
Run: `grep -rn "compose_inline\|build_prompt" apps/desktop/src-tauri/src`
Expected: 呼び出し箇所が `user_cfg.directives()` を渡している。

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/src/user_config_watch.rs apps/desktop/src-tauri/src/lib.rs Cargo.lock
git commit -m "feat(user-config): ~/Shougun.md の監視と共有状態 (#41)"
```

---

## Task 9: Tauri commands（status / open / regenerate）

**Files:**
- Modify: `apps/desktop/src-tauri/src/user_config_watch.rs`（commands 追加）
- Modify: `apps/desktop/src-tauri/src/lib.rs`（`generate_handler!` に登録）

- [ ] **Step 1: command と DTO を追加**

`user_config_watch.rs` の末尾（`#[cfg(test)]` の外）に追加:
```rust
use shogun_core::user_config::{ParseReport, SectionError};

#[derive(serde::Serialize)]
pub struct UserConfigStatus {
    pub exists: bool,
    pub path: String,
    pub last_updated_ms: Option<u64>,
    pub ok: bool,
    pub errors: Vec<SectionErrorDto>,
}

#[derive(serde::Serialize)]
pub struct SectionErrorDto {
    pub section: String,
    pub line: usize,
    pub message: String,
}

impl From<SectionError> for SectionErrorDto {
    fn from(e: SectionError) -> Self {
        SectionErrorDto { section: e.section, line: e.line, message: e.message }
    }
}

fn resolved_path() -> Result<std::path::PathBuf, String> {
    default_path().ok_or_else(|| "could not resolve home dir".to_string())
}

#[tauri::command]
pub fn get_user_config_status() -> Result<UserConfigStatus, String> {
    let path = resolved_path()?;
    let exists = path.exists();
    let last_updated_ms = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    let (report, ok): (ParseReport, bool) = if exists {
        let (_c, r) = load_report(&path).map_err(|e| e.to_string())?;
        let ok = r.ok;
        (r, ok)
    } else {
        (ParseReport { ok: true, section_errors: vec![] }, true)
    };
    Ok(UserConfigStatus {
        exists,
        path: path.to_string_lossy().to_string(),
        last_updated_ms,
        ok,
        errors: report.section_errors.into_iter().map(Into::into).collect(),
    })
}

#[tauri::command]
pub fn open_shougun_md() -> Result<(), String> {
    let path = resolved_path()?;
    std::process::Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("failed to open: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn regenerate_shougun_md(state: tauri::State<'_, UserConfigState>) -> Result<(), String> {
    use shogun_core::user_config::{parse_shougun, sample_markdown};
    let path = resolved_path()?;
    let sample = sample_markdown();
    std::fs::write(&path, &sample).map_err(|e| e.to_string())?;
    let (cfg, _r) = parse_shougun(&sample);
    if let Ok(mut w) = state.cfg.write() {
        *w = cfg;
    }
    Ok(())
}
```

- [ ] **Step 2: command を登録**

`apps/desktop/src-tauri/src/lib.rs` の `tauri::generate_handler![ ... ]` に追加:
```rust
        user_config_watch::get_user_config_status,
        user_config_watch::open_shougun_md,
        user_config_watch::regenerate_shougun_md,
```

- [ ] **Step 3: ビルド確認**

Run: ワークスペースルートで `cargo build`
Expected: 成功

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src-tauri/src/user_config_watch.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(user-config): 設定状態/オープン/再生成の Tauri command (#41)"
```

---

## Task 10: PersonalizationSection UI

**Files:**
- Modify: `apps/desktop/src/App.tsx`（`PersonalizationSection` 追加＋`Settings` へ挿入）
- Modify: `apps/desktop/src/styles.css`（`.set__hint.is-err` 追加。既存なら不要）

- [ ] **Step 1: コンポーネントと型を追加**

`App.tsx`（`AiSessionsSection` 付近）に追加:
```tsx
interface UserConfigStatus {
  exists: boolean;
  path: string;
  last_updated_ms: number | null;
  ok: boolean;
  errors: { section: string; line: number; message: string }[];
}

function PersonalizationSection(): JSX.Element {
  const [status, setStatus] = useState<UserConfigStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const refresh = (): void => {
    if (!IN_TAURI) return;
    void invoke<UserConfigStatus>("get_user_config_status").then(setStatus).catch(() => undefined);
  };
  useEffect(refresh, []);

  return (
    <section className="set">
      <div className="set__label">Personalization / Shougun.md</div>
      <div className="set__hint">Shape ShogunAI with one human-readable Markdown file.</div>
      {status ? (
        <>
          <div className={`set__hint${status.ok ? " is-ok" : " is-err"}`}>
            {status.exists
              ? status.ok
                ? "Parsed successfully"
                : `Parse error: ${status.errors[0]?.section ?? ""} (line ${status.errors[0]?.line ?? 0})`
              : "Not created yet"}
          </div>
          <div className="set__row">
            <button
              className="keyrow__btn"
              type="button"
              disabled={!status.exists || busy}
              onClick={() => void invoke("open_shougun_md").catch((e) => setErr(String(e)))}
            >
              Open in Editor
            </button>
            <button
              className="keyrow__btn"
              type="button"
              disabled={busy}
              onClick={() => {
                setBusy(true);
                void invoke("regenerate_shougun_md")
                  .then(refresh)
                  .catch((e) => setErr(String(e)))
                  .finally(() => setBusy(false));
              }}
            >
              Regenerate Sample
            </button>
          </div>
          {err ? <div className="set__hint is-err">{err}</div> : null}
        </>
      ) : null}
    </section>
  );
}
```

- [ ] **Step 2: Settings に挿入**

`App.tsx` の `Settings` 内、`<DreamSection />` の直後に追加:
```tsx
      <PersonalizationSection />
```

- [ ] **Step 3: CSS を追加（`.set__hint.is-err` が無ければ）**

`apps/desktop/src/styles.css` の `.set__hint.is-ok` の下に追加:
```css
.set__hint.is-err {
  color: var(--warn);
}
```

- [ ] **Step 4: 型チェック**

Run: `pnpm -C apps/desktop typecheck`（無ければ `pnpm -C apps/desktop exec tsc --noEmit`）
Expected: エラーなし

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/App.tsx apps/desktop/src/styles.css
git commit -m "feat(user-config): 設定画面に Personalization セクション (#41)"
```

---

## Task 11: CLI `config` サブコマンド

**Files:**
- Modify: `crates/shogun-cli/src/command.rs`（`Command::Config`・`ConfigAction`・`USAGE`）
- Modify: `crates/shogun-cli/src/parse.rs`（`config` のパース）
- Modify: `crates/shogun-cli/src/main.rs`（ローカル解決の分岐）

> `config` は daemon を介さずローカルの `~/Shougun.md` を直接読む（`shogun_core::user_config` を利用）。CLI crate に `shogun-core` 依存が無ければ `crates/shogun-cli/Cargo.toml` に `shogun-core = { path = "../shogun-core" }` を追加する。

- [ ] **Step 1: パーサの失敗するテストを書く**

`crates/shogun-cli/src/parse.rs` の `#[cfg(test)] mod tests` に追加:
```rust
    #[test]
    fn parses_config_actions() {
        assert_eq!(
            parse(&["config".into(), "validate".into()]).unwrap().command,
            Command::Config { action: ConfigAction::Validate }
        );
        assert_eq!(
            parse(&["config".into(), "path".into()]).unwrap().command,
            Command::Config { action: ConfigAction::Path }
        );
    }
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cargo test -p shogun-cli parses_config_actions`
Expected: FAIL（`Command::Config` / `ConfigAction` 未定義）

- [ ] **Step 3: 型とパースを実装**

`command.rs` の `Command` enum に追加:
```rust
    /// `shogun config path|show|validate`
    Config { action: ConfigAction },
```
同ファイルに追加:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigAction {
    Path,
    Show,
    Validate,
}
```
`tool()` の match に追加（daemon ツールを使わないため None）:
```rust
            Command::Config { .. } => return None,
```
`USAGE` 文字列に 1 行追記:
```
  config path|show|validate   Shougun.md の場所/内容/検証
```

`parse.rs` の `config` 分岐（`match positionals.first()` の腕として）:
```rust
        Some("config") => {
            let action = match positionals.get(1).map(String::as_str) {
                Some("path") => ConfigAction::Path,
                Some("show") => ConfigAction::Show,
                Some("validate") => ConfigAction::Validate,
                Some(other) => {
                    return Err(CliError::UnknownSubcommand {
                        command: "config",
                        got: other.to_string(),
                    })
                }
                None => return Err(CliError::MissingArgument("config action")),
            };
            Ok(Command::Config { action })
        }
```
> `CliError` のバリアント名は既存に合わせる（`UnknownSubcommand` / `MissingArgument` が無ければ既存の最も近いエラー型を使う）。

- [ ] **Step 4: main.rs にローカル分岐を追加**

`crates/shogun-cli/src/main.rs`、`Help` 分岐の直後に追加:
```rust
    if let Command::Config { action } = &invocation.command {
        use shogun_core::user_config::{default_path, load_report};
        let Some(path) = default_path() else {
            eprintln!("error: could not resolve home dir");
            return ExitCode::from(1);
        };
        match action {
            command::ConfigAction::Path => {
                println!("{}", path.display());
            }
            command::ConfigAction::Show => match load_report(&path) {
                Ok((cfg, _)) => println!("{:#?}", cfg),
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(1);
                }
            },
            command::ConfigAction::Validate => match load_report(&path) {
                Ok((_, report)) => {
                    if report.ok {
                        println!("ok");
                    } else {
                        for e in &report.section_errors {
                            println!("{}:{} {}", e.section, e.line, e.message);
                        }
                        return ExitCode::from(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(1);
                }
            },
        }
        return ExitCode::SUCCESS;
    }
```

- [ ] **Step 5: テストとビルドを確認**

Run: `cargo test -p shogun-cli`
Expected: PASS（`parses_config_actions` を含む）
Run: `cargo build -p shogun-cli`
Expected: 成功

- [ ] **Step 6: Commit**

```bash
git add crates/shogun-cli/src/command.rs crates/shogun-cli/src/parse.rs crates/shogun-cli/src/main.rs crates/shogun-cli/Cargo.toml Cargo.lock
git commit -m "feat(user-config): CLI config path|show|validate (#41)"
```

---

## Task 12: 残りの user-facing プロンプト経路の配線

**Files:**
- Modify: 検索で判明した user-facing 生成の system prompt 組み立て箇所（chat / Morning Brief / 紹介文生成）

> Task 7 は inline（at-cursor）だけを配線した。chat・Morning Brief・紹介文生成が別の関数で system prompt を組んでいる場合、同じ `render_directives` を先頭に差し込む。ここは実コードの所在を確認してから配線する。

- [ ] **Step 1: 生成箇所を洗い出す**

Run: `grep -rn "system\|You are\|push_str(\"You" crates/shogun-fusion/src crates/shogun-core/src apps/desktop/src-tauri/src --include=*.rs | grep -iv test`
Run: `grep -rn "morning\|brief\|chat\|intro" crates apps --include=*.rs | grep -i prompt`
目的: chat / Morning Brief / 紹介文の system prompt を組む関数を特定する。

- [ ] **Step 2: 各箇所へ directives を先頭連結**

各関数で、system prompt 文字列を組む先頭に以下を挿入する（`cfg` は `UserConfigState::directives()` あるいは呼び出しコンテキストの `ShougunConfig` から取得）:
```rust
let directives = /* UserConfigState.directives() もしくは render_directives(&cfg) */;
if !directives.trim().is_empty() {
    prompt.push_str(&directives);
    prompt.push('\n');
}
```
背景処理（indexing / Dream 分類）には**差し込まない**。

- [ ] **Step 3: ビルドと全テスト**

Run: ワークスペースルートで `cargo test`
Expected: PASS
Run: `cargo build`
Expected: 成功

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(user-config): chat/Morning Brief への directives 配線 (#41)"
```

---

## 設計書からの実装上の差分（refinement）

- パーサは `pulldown-cmark` ではなく**行ベース**（見出し `# ` 分割＋`- Key: value`）。この単純フォーマットでは行ベースの方が堅牢・テスト容易・依存最小。
- 設計の「bus event を emit」は**採用しない**（①に内部 consumer が無い＝YAGNI）。設定は `render_directives` 呼び出し時に on-demand で読む。設定画面のステータスは「開いた時＋再生成後」に更新。
- **①-5 の MCP/REST read は本 Issue のスコープ外に後退**。AI/ファイル文化向けの対称性は CLI `shogun config`（Task 11）で v1 を満たす。MCP ツール化は `shogun-mcp` の構造調査が要るため、後続 Issue で扱う（サイレントに落とさず、ここで明示）。

## 完了条件

- `cargo test`（ワークスペース）と `cargo build` が通る。
- `pnpm -C apps/desktop typecheck` が通る。
- `~/Shougun.md` を編集すると数秒以内に再パースされ、以降の user-facing 生成に反映される。
- 設定画面に Personalization セクションが表示され、Open / Regenerate / ステータスが機能する。
- `shogun config path|show|validate` が動作する。
- `# Charm` が不正でもコア機能は継続し、Charm のみ無効化される（fail-soft）。
