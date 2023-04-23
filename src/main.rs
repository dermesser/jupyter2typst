use markdown::mdast::Node;
use rustop::opts;
use tinyjson::JsonValue;

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::{fs, path::Path};

struct Context {
    verbose: bool,
}

fn notebook_overview(ctx: &Context, nb: &JsonValue) {
    match nb {
        JsonValue::Object(ref hm) => {
            if ctx.verbose {
                println!("Notebook with keys {:?}", hm.keys());
                println!(
                    "Version: {}.{}",
                    hm["nbformat"].format().unwrap(),
                    hm["nbformat_minor"].format().unwrap()
                );
                println!("=> Well-formed!");

                let md: HashMap<_, _> = hm["metadata"].clone().try_into().unwrap();
                println!("Language: {}", md["kernelspec"].format().unwrap());
            }
        }
        _ => {
            println!("Unknown notebook format!");
        }
    };
}

fn parse_notebook_file<S: AsRef<Path>>(filename: S) -> JsonValue {
    let file = fs::read(filename).unwrap();
    let val: JsonValue = String::from_utf8(file).unwrap().parse().unwrap();
    val
}

const document_root: &str = r###"
#let input_notebook = "database_and_analysis.ipynb"

#let sanitize_markdown(md) = md.replace("#", "=").replace("= ", "=")

#let bgcolor_code = luma(230)
#let bgcolor_result = rgb("a7d1de")
#let codeblock(
    lang: "python",
    bgcolor: luma(230),
    code) = block(fill: bgcolor,
                  outset: 5pt,
                  radius: 3pt,
                  width: 100%,
                  raw(code, lang: lang))


"###;

#[derive(Debug, Default)]
enum J2TErrorKind {
    Json(tinyjson::UnexpectedValue),
    Md(String),
    #[default]
    Unknown,
}

#[derive(Debug, Default)]
struct J2TError {
    kind: J2TErrorKind,
    msg: Option<String>,
}

impl std::fmt::Display for J2TError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_fmt(format_args!(
            "{} ({:?})",
            self.msg.as_deref().unwrap_or(""),
            self.kind
        ))
    }
}

impl Error for J2TError {}

impl From<String> for J2TError {
    fn from(s: String) -> J2TError {
        J2TError {
            kind: J2TErrorKind::Md(s),
            ..Default::default()
        }
    }
}
impl From<tinyjson::UnexpectedValue> for J2TError {
    fn from(s: tinyjson::UnexpectedValue) -> J2TError {
        J2TError {
            kind: J2TErrorKind::Json(s),
            ..Default::default()
        }
    }
}

fn markdown_to_typst(n: &Node) -> String {
    match n {
        // TODO: implement markdown-to-typst translation
        Node::Root(ref r) => {
            let chs = r.children.iter().map(markdown_to_typst).collect::<Vec<_>>();
            chs.join(" ")
        }
        _ => String::new(),
    }
}

fn convert_markdown_to_typst(s: &str) -> Result<String, J2TError> {
    let po = markdown::ParseOptions::default();
    let ast = markdown::to_mdast(s, &po)?;
    println!("{:?}", ast);
    Ok(markdown_to_typst(&ast))
}

fn format_cell(cell: &JsonValue) -> Result<String, J2TError> {
    let hm: HashMap<_, _> = cell.clone().try_into()?;

    if String::try_from(hm["cell_type"].clone()).unwrap() == "markdown" {
        let joined: String =
            <Vec<JsonValue> as TryFrom<JsonValue>>::try_from(hm["source"].clone())?
                .into_iter()
                .map(|s| <JsonValue as TryInto<String>>::try_into(s).unwrap())
                .collect::<Vec<String>>()
                .join("");
        convert_markdown_to_typst(&joined)
    } else {
        Ok(String::new())
    }
}

fn main() {
    let (args, _rest) = opts! {
        synopsis "Convert a jupyter notebook into typst source code.";
        opt verbose:bool, desc:"Enable verbosity";
        param file:String, desc:"Input file name";
    }
    .parse_or_exit();

    let ctx = Context {
        verbose: args.verbose,
    };
    let parsed_json = parse_notebook_file(args.file);
    notebook_overview(&ctx, &parsed_json);

    let parsed_dict = <HashMap<_, _>>::try_from(parsed_json).unwrap();

    let cells =
        <Vec<JsonValue> as TryFrom<JsonValue>>::try_from(parsed_dict["cells"].clone()).unwrap();

    format_cell(&cells[0]).expect("format failed");
}
