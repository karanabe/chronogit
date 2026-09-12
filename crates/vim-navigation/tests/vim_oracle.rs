//! Executable comparison against the Vim release documented in the crate guide.
//!
//! This test is ignored in the ordinary workspace suite because Vim is not a
//! build dependency. Run it explicitly with a compatible binary:
//!
//! `VIM_ORACLE=/path/to/vim cargo test -p vim-navigation --test vim_oracle -- --ignored`

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use vim_navigation::{CountSource, Cursor, Motion, MotionKind, Viewport, apply};

const HEIGHT: usize = 23;
const WIDTH: usize = 80;
const FIXTURE: &str = concat!(
    "  alpha,beta  GAMMA\n",
    "short\n",
    "\twide 界 text\n",
    "\n",
    "First sentence. Next one! Last?\n",
    "\n",
    "paragraph two\n",
    "line two\n",
    "{\n",
    "section start\n",
    "}\n",
    "class Sample {\n",
    "  fn first() {\n",
    "    body();\n",
    "  }\n",
    "  fn second() {\n",
    "    value = (left[right]);\n",
    "  }\n",
    "}\n",
    "#if OUTER\n",
    "outer\n",
    "#else\n",
    "/* comment\n",
    "body */\n",
    "#endif\n",
    "-old\n",
    "+new\n",
    " context\n",
    "-older\n",
    "+newer\n",
    "012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789\n",
    "line 31\n",
    "line 32\n",
    "line 33\n",
    "line 34\n",
    "line 35\n",
    "line 36\n",
    "line 37\n",
    "line 38\n",
    "line 39\n",
    "line 40\n",
    "line 41\n",
    "line 42\n",
    "line 43\n",
    "line 44\n",
    "line 45",
);

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    fixture: &'static str,
    keys: &'static str,
    cursor: Cursor,
    top: usize,
    left: usize,
    motion: Motion,
    options: &'static str,
    compare_viewport: bool,
}

impl Case {
    const fn new(name: &'static str, keys: &'static str, cursor: Cursor, motion: Motion) -> Self {
        Self {
            name,
            fixture: FIXTURE,
            keys,
            cursor,
            top: 0,
            left: 0,
            motion,
            options: "",
            compare_viewport: false,
        }
    }

    const fn view(mut self, top: usize, left: usize) -> Self {
        self.top = top;
        self.left = left;
        self.compare_viewport = true;
        self
    }

    const fn options(mut self, options: &'static str) -> Self {
        self.options = options;
        self
    }

    const fn fixture(mut self, fixture: &'static str) -> Self {
        self.fixture = fixture;
        self
    }
}

const fn counted(kind: MotionKind, count: usize) -> Motion {
    Motion::new(kind).counted(count, CountSource::Explicit)
}

const fn targeted(kind: MotionKind, count: usize, target: char) -> Motion {
    counted(kind, count).targeting(target)
}

#[test]
#[ignore = "requires the separately supplied Vim 9.1.1244 oracle"]
fn vim_9_1_1244_cursor_and_viewport_results_match() {
    let Some(vim) = env::var_os("VIM_ORACLE") else {
        panic!("set VIM_ORACLE to a Vim 9.1.1244 executable");
    };
    assert_oracle_version(&vim);

    let cases = [
        Case::new(
            "sentence forward after closing punctuation",
            ")",
            Cursor::new(0, 0),
            Motion::new(MotionKind::SentenceForward),
        )
        .fixture("First.\")] Next.\nAnother."),
        Case::new(
            "sentence backward after closing punctuation",
            "(",
            Cursor::new(1, 0),
            Motion::new(MotionKind::SentenceBackward),
        )
        .fixture("First.\")] Next.\nAnother."),
        Case::new("h", "3h", Cursor::new(0, 10), counted(MotionKind::Left, 3)),
        Case::new("l", "4l", Cursor::new(0, 2), counted(MotionKind::Right, 4)),
        Case::new(
            "Backspace whichwrap",
            "2\\<BS>",
            Cursor::new(1, 0),
            counted(MotionKind::LeftWrap, 2),
        )
        .options("set whichwrap=b"),
        Case::new(
            "Space whichwrap",
            "2\\<Space>",
            Cursor::new(0, 18),
            counted(MotionKind::RightWrap, 2),
        )
        .options("set whichwrap=s"),
        Case::new("k", "2k", Cursor::new(2, 6), counted(MotionKind::Up, 2)),
        Case::new("j", "2j", Cursor::new(0, 6), counted(MotionKind::Down, 2)),
        Case::new(
            "gk nowrap",
            "2gk",
            Cursor::new(2, 6),
            counted(MotionKind::Up, 2),
        ),
        Case::new(
            "gj nowrap",
            "2gj",
            Cursor::new(0, 6),
            counted(MotionKind::Down, 2),
        ),
        Case::new(
            "0",
            "0",
            Cursor::new(0, 10),
            Motion::new(MotionKind::LineStart),
        ),
        Case::new(
            "^",
            "^",
            Cursor::new(0, 10),
            Motion::new(MotionKind::FirstNonBlank),
        ),
        Case::new(
            "$",
            "2$",
            Cursor::new(0, 2),
            counted(MotionKind::LineEnd, 2),
        ),
        Case::new(
            "g_",
            "2g_",
            Cursor::new(0, 2),
            counted(MotionKind::LastNonBlank, 2),
        ),
        Case::new(
            "g0 nowrap",
            "g0",
            Cursor::new(30, 90),
            Motion::new(MotionKind::ScreenLineStart),
        )
        .view(19, 40),
        Case::new(
            "g^ nowrap",
            "g^",
            Cursor::new(30, 90),
            Motion::new(MotionKind::ScreenFirstNonBlank),
        )
        .view(19, 40),
        Case::new(
            "g$ nowrap",
            "g$",
            Cursor::new(30, 50),
            Motion::new(MotionKind::ScreenLineEnd),
        )
        .view(19, 40),
        Case::new(
            "gEnd nowrap",
            "g\\<End>",
            Cursor::new(30, 50),
            Motion::new(MotionKind::ScreenLastNonBlank),
        )
        .view(19, 40),
        Case::new(
            "gm nowrap",
            "gm",
            Cursor::new(30, 50),
            Motion::new(MotionKind::ScreenMiddle),
        )
        .view(19, 40),
        Case::new(
            "gM",
            "25gM",
            Cursor::new(30, 50),
            counted(MotionKind::LineMiddle, 25),
        ),
        Case::new(
            "bar",
            "9|",
            Cursor::new(2, 0),
            counted(MotionKind::Column, 9),
        ),
        Case::new(
            "go",
            "35go",
            Cursor::new(10, 0),
            counted(MotionKind::ByteOffset, 35),
        ),
        Case::new(
            "w",
            "3w",
            Cursor::new(0, 2),
            counted(MotionKind::WordForward, 3),
        ),
        Case::new(
            "W",
            "2W",
            Cursor::new(0, 2),
            counted(MotionKind::BigWordForward, 2),
        ),
        Case::new(
            "e",
            "3e",
            Cursor::new(0, 2),
            counted(MotionKind::WordEndForward, 3),
        ),
        Case::new(
            "E",
            "2E",
            Cursor::new(0, 2),
            counted(MotionKind::BigWordEndForward, 2),
        ),
        Case::new(
            "b",
            "2b",
            Cursor::new(0, 15),
            counted(MotionKind::WordBackward, 2),
        ),
        Case::new(
            "B",
            "2B",
            Cursor::new(0, 15),
            counted(MotionKind::BigWordBackward, 2),
        ),
        Case::new(
            "ge",
            "2ge",
            Cursor::new(0, 15),
            counted(MotionKind::WordEndBackward, 2),
        ),
        Case::new(
            "gE",
            "2gE",
            Cursor::new(0, 15),
            counted(MotionKind::BigWordEndBackward, 2),
        ),
        Case::new(
            "f",
            "2fa",
            Cursor::new(0, 0),
            targeted(MotionKind::FindForward, 2, 'a'),
        ),
        Case::new(
            "F",
            "2Fa",
            Cursor::new(0, 17),
            targeted(MotionKind::FindBackward, 2, 'a'),
        ),
        Case::new(
            "t",
            "2ta",
            Cursor::new(0, 0),
            targeted(MotionKind::TillForward, 2, 'a'),
        ),
        Case::new(
            "T",
            "2Ta",
            Cursor::new(0, 17),
            targeted(MotionKind::TillBackward, 2, 'a'),
        ),
        Case::new(
            "minus",
            "2-",
            Cursor::new(7, 5),
            counted(MotionKind::PreviousLineFirstNonBlank, 2),
        ),
        Case::new(
            "plus",
            "2+",
            Cursor::new(6, 5),
            counted(MotionKind::NextLineFirstNonBlank, 2),
        ),
        Case::new(
            "underscore",
            "3_",
            Cursor::new(6, 5),
            counted(MotionKind::CountedLineFirstNonBlank, 3),
        ),
        Case::new(
            "gg",
            "gg",
            Cursor::new(20, 2),
            Motion::new(MotionKind::BufferTop),
        ),
        Case::new(
            "G",
            "G",
            Cursor::new(2, 2),
            Motion::new(MotionKind::BufferBottom),
        ),
        Case::new(
            "Ctrl-End",
            "\\<C-End>",
            Cursor::new(2, 2),
            Motion::new(MotionKind::BufferBottomEnd),
        ),
        Case::new(
            "percentage",
            "40%",
            Cursor::new(0, 2),
            counted(MotionKind::BufferPercentage, 40),
        ),
        Case::new(
            "H",
            "3H",
            Cursor::new(20, 2),
            counted(MotionKind::WindowTop, 3),
        )
        .view(10, 0),
        Case::new(
            "M",
            "M",
            Cursor::new(20, 2),
            Motion::new(MotionKind::WindowMiddle),
        )
        .view(10, 0),
        Case::new(
            "L",
            "2L",
            Cursor::new(20, 2),
            counted(MotionKind::WindowBottom, 2),
        )
        .view(10, 0),
        Case::new(
            "sentence backward",
            "(",
            Cursor::new(4, 28),
            Motion::new(MotionKind::SentenceBackward),
        ),
        Case::new(
            "sentence forward",
            "2)",
            Cursor::new(4, 0),
            counted(MotionKind::SentenceForward, 2),
        ),
        Case::new(
            "paragraph backward",
            "{",
            Cursor::new(7, 4),
            Motion::new(MotionKind::ParagraphBackward),
        ),
        Case::new(
            "paragraph forward",
            "}",
            Cursor::new(6, 4),
            Motion::new(MotionKind::ParagraphForward),
        ),
        Case::new(
            "section [[",
            "[[",
            Cursor::new(11, 3),
            Motion::new(MotionKind::SectionStartBackward),
        ),
        Case::new(
            "section ]]",
            "]]",
            Cursor::new(7, 3),
            Motion::new(MotionKind::SectionStartForward),
        ),
        Case::new(
            "section []",
            "[]",
            Cursor::new(18, 0),
            Motion::new(MotionKind::SectionEndBackward),
        ),
        Case::new(
            "section ][",
            "][",
            Cursor::new(7, 3),
            Motion::new(MotionKind::SectionEndForward),
        ),
        Case::new(
            "matching pair",
            "%",
            Cursor::new(16, 12),
            Motion::new(MotionKind::MatchingPair),
        ),
        Case::new(
            "unmatched [( ",
            "[(",
            Cursor::new(16, 24),
            targeted(MotionKind::UnmatchedOpenBackward, 1, '('),
        ),
        Case::new(
            "unmatched [{",
            "[{",
            Cursor::new(16, 24),
            targeted(MotionKind::UnmatchedOpenBackward, 1, '{'),
        ),
        Case::new(
            "unmatched ])",
            "])",
            Cursor::new(16, 12),
            targeted(MotionKind::UnmatchedCloseForward, 1, ')'),
        ),
        Case::new(
            "unmatched ]}",
            "]}",
            Cursor::new(12, 13),
            targeted(MotionKind::UnmatchedCloseForward, 1, '}'),
        ),
        Case::new(
            "method [m",
            "[m",
            Cursor::new(16, 18),
            Motion::new(MotionKind::MethodBackward),
        ),
        Case::new(
            "method [M",
            "[M",
            Cursor::new(16, 18),
            Motion::new(MotionKind::MethodBackward).targeting('M'),
        ),
        Case::new(
            "method ]m",
            "]m",
            Cursor::new(12, 2),
            Motion::new(MotionKind::MethodForward),
        ),
        Case::new(
            "method ]M",
            "]M",
            Cursor::new(12, 2),
            Motion::new(MotionKind::MethodForward).targeting('M'),
        ),
        Case::new(
            "preprocessor [#",
            "[#",
            Cursor::new(22, 2),
            Motion::new(MotionKind::PreprocessorBackward),
        ),
        Case::new(
            "preprocessor ]#",
            "]#",
            Cursor::new(20, 2),
            Motion::new(MotionKind::PreprocessorForward),
        ),
        Case::new(
            "comment [/",
            "[/",
            Cursor::new(23, 3),
            Motion::new(MotionKind::CommentBackward),
        ),
        Case::new(
            "comment ]/",
            "]/",
            Cursor::new(22, 2),
            Motion::new(MotionKind::CommentForward),
        ),
        Case::new(
            "Ctrl-D",
            "\\<C-D>",
            Cursor::new(15, 3),
            Motion::new(MotionKind::HalfPageDown),
        )
        .view(5, 0),
        Case::new(
            "Ctrl-U",
            "\\<C-U>",
            Cursor::new(25, 3),
            Motion::new(MotionKind::HalfPageUp),
        )
        .view(15, 0),
        Case::new(
            "Ctrl-F",
            "\\<C-F>",
            Cursor::new(15, 3),
            Motion::new(MotionKind::PageDown),
        )
        .view(5, 0),
        Case::new(
            "Ctrl-B",
            "\\<C-B>",
            Cursor::new(25, 3),
            Motion::new(MotionKind::PageUp),
        )
        .view(15, 0),
        Case::new(
            "Ctrl-E",
            "2\\<C-E>",
            Cursor::new(15, 3),
            counted(MotionKind::ScrollLineDown, 2),
        )
        .view(5, 0),
        Case::new(
            "Ctrl-Y",
            "2\\<C-Y>",
            Cursor::new(25, 3),
            counted(MotionKind::ScrollLineUp, 2),
        )
        .view(15, 0),
        Case::new(
            "zt",
            "zt",
            Cursor::new(20, 3),
            Motion::new(MotionKind::CursorToWindowTop),
        )
        .view(10, 0),
        Case::new(
            "zEnter",
            "z\\<CR>",
            Cursor::new(20, 3),
            Motion::new(MotionKind::CursorToWindowTopFirstNonBlank),
        )
        .view(10, 0),
        Case::new(
            "zz",
            "zz",
            Cursor::new(20, 3),
            Motion::new(MotionKind::CursorToWindowMiddle),
        )
        .view(10, 0),
        Case::new(
            "z.",
            "z.",
            Cursor::new(20, 3),
            Motion::new(MotionKind::CursorToWindowMiddleFirstNonBlank),
        )
        .view(10, 0),
        Case::new(
            "zb",
            "zb",
            Cursor::new(20, 3),
            Motion::new(MotionKind::CursorToWindowBottom),
        )
        .view(10, 0),
        Case::new(
            "z-",
            "z-",
            Cursor::new(20, 3),
            Motion::new(MotionKind::CursorToWindowBottomFirstNonBlank),
        )
        .view(10, 0),
        Case::new(
            "z+",
            "z+",
            Cursor::new(15, 3),
            Motion::new(MotionKind::NextWindowTop),
        )
        .view(5, 0),
        Case::new(
            "z^",
            "z^",
            Cursor::new(25, 3),
            Motion::new(MotionKind::PreviousWindowBottom),
        )
        .view(15, 0),
        Case::new(
            "zl",
            "3zl",
            Cursor::new(30, 70),
            counted(MotionKind::ScrollColumnRight, 3),
        )
        .view(19, 20),
        Case::new(
            "zh",
            "3zh",
            Cursor::new(30, 70),
            counted(MotionKind::ScrollColumnLeft, 3),
        )
        .view(19, 20),
        Case::new(
            "zL",
            "zL",
            Cursor::new(30, 70),
            Motion::new(MotionKind::ScrollHalfScreenRight),
        )
        .view(19, 20),
        Case::new(
            "zH",
            "zH",
            Cursor::new(30, 70),
            Motion::new(MotionKind::ScrollHalfScreenLeft),
        )
        .view(19, 20),
        Case::new(
            "zs",
            "zs",
            Cursor::new(30, 70),
            Motion::new(MotionKind::CursorToWindowLeft),
        )
        .view(19, 20),
        Case::new(
            "ze",
            "ze",
            Cursor::new(30, 90),
            Motion::new(MotionKind::CursorToWindowRight),
        )
        .view(19, 20),
    ];

    let selected = env::var("VIM_ORACLE_CASE").ok();
    let mut compared = 0usize;
    for case in cases {
        if selected
            .as_deref()
            .is_some_and(|filter| !case.name.contains(filter))
        {
            continue;
        }
        compare_case(&vim, case);
        compared = compared.saturating_add(1);
    }
    assert!(compared > 0, "VIM_ORACLE_CASE did not match an oracle case");
    if selected.is_none() {
        assert_eq!(
            compared, 85,
            "the complete Vim oracle inventory changed; update the compatibility evidence deliberately"
        );
    }
    eprintln!("compared {compared} Vim cursor/viewport cases");
}

fn assert_oracle_version(vim: &OsStr) {
    let output = match Command::new(vim)
        .env("LC_ALL", "C")
        .arg("--version")
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("could not run VIM_ORACLE: {error}"),
    };
    let version = String::from_utf8_lossy(&output.stdout);
    let exact_patch_set = version
        .lines()
        .any(|line| line.trim() == "Included patches: 1-1244");
    assert!(
        output.status.success() && version.contains("VIM - Vi IMproved 9.1") && exact_patch_set,
        "VIM_ORACLE must be Vim 9.1.1244, got:\n{version}"
    );
}

fn compare_case(vim: &OsStr, case: Case) {
    let directory = temporary_directory(case.name);
    let input = directory.join("fixture.txt");
    let output = directory.join("position.txt");
    let script = directory.join("oracle.vim");
    write(&input, case.fixture);
    write(&script, &vim_script(case, &output));

    let invocation = format!(
        "stty rows 24 columns 80 && exec {} -Nu NONE -n -i NONE {} -S {}",
        shell_word(Path::new(vim)),
        shell_word(&input),
        shell_word(&script),
    );
    let mut command = Command::new("script");
    // macOS ships BSD script; Linux uses util-linux's different CLI.
    if cfg!(target_os = "macos") {
        command.args(["-q", "/dev/null", "sh", "-c", &invocation]);
    } else {
        command.args(["-qefc", &invocation, "/dev/null"]);
    }
    let result = match command
        .env("LC_ALL", "C")
        .env("TERM", "xterm")
        .env("LINES", "24")
        .env("COLUMNS", "80")
        .output()
    {
        Ok(result) => result,
        Err(error) => panic!(
            "{}: could not run Vim in a pseudo-terminal: {error}",
            case.name
        ),
    };
    assert!(
        result.status.success(),
        "{}: Vim failed: {}",
        case.name,
        String::from_utf8_lossy(&result.stderr)
    );
    let actual = read_position(&output, case.name);

    let lines = case.fixture.lines().collect::<Vec<_>>();
    let mut viewport = Viewport::new(case.top, case.left, HEIGHT, WIDTH, 0);
    let cursor = apply(&lines, case.cursor, &mut viewport, case.motion);
    assert_eq!(
        (cursor.line(), cursor.byte_column()),
        (actual[0], actual[1]),
        "{} cursor",
        case.name
    );
    if case.compare_viewport {
        assert_eq!(
            (viewport.top(), viewport.left()),
            (actual[2], actual[3]),
            "{} viewport",
            case.name
        );
    }

    if let Err(error) = fs::remove_dir_all(&directory) {
        panic!(
            "{}: could not remove {}: {error}",
            case.name,
            directory.display()
        );
    }
}

fn shell_word(path: &Path) -> String {
    format!(
        "'{}'",
        path.to_string_lossy().replace(char::from(39), "'\\''")
    )
}

fn vim_script(case: Case, output: &Path) -> String {
    let output = output.to_string_lossy().replace(char::from(39), "''");
    format!(
        concat!(
            "set nocompatible\n",
            "set encoding=utf-8 fileformat=unix nowrap nofoldenable\n",
            "set scrolloff=0 sidescrolloff=0 virtualedit= startofline tabstop=4\n",
            "set cpoptions&vim\n",
            "set lines=24 columns=80\n",
            "{}\n",
            "call winrestview({{'lnum': {}, 'col': {}, 'topline': {}, 'leftcol': {}}})\n",
            "redraw\n",
            "execute \"normal! {}\"\n",
            "let s:view = winsaveview()\n",
            "call writefile([printf('%d,%d,%d,%d', line('.') - 1, col('.') - 1, s:view.topline - 1, s:view.leftcol)], '{}')\n",
            "qa!\n",
        ),
        case.options,
        case.cursor.line().saturating_add(1),
        case.cursor.byte_column(),
        case.top.saturating_add(1),
        case.left,
        case.keys,
        output,
    )
}

fn read_position(path: &Path, name: &str) -> [usize; 4] {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => panic!("{name}: could not read {}: {error}", path.display()),
    };
    let values = source
        .trim()
        .split(',')
        .map(str::parse::<usize>)
        .collect::<Result<Vec<_>, _>>();
    match values {
        Ok(values) if values.len() == 4 => [values[0], values[1], values[2], values[3]],
        Ok(values) => panic!("{name}: expected four Vim position values, got {values:?}"),
        Err(error) => panic!("{name}: invalid Vim position output: {error}"),
    }
}

fn write(path: &Path, contents: &str) {
    if let Err(error) = fs::write(path, contents) {
        panic!("could not write {}: {error}", path.display());
    }
}

fn temporary_directory(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    let safe_name = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(error) => panic!("system clock is before the Unix epoch: {error}"),
    };
    let path = env::temp_dir().join(format!(
        "vim-navigation-oracle-{}-{timestamp}-{serial}-{safe_name}",
        std::process::id(),
    ));
    if let Err(error) = fs::create_dir(&path) {
        panic!("could not create {}: {error}", path.display());
    }
    path
}
