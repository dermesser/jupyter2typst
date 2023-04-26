use markdown::mdast::Node;
use rustop::opts;
use tinyjson::JsonValue;

use std::collections::{
    hash_map::{Entry, OccupiedEntry},
    HashMap,
};
use std::error::Error;
use std::fmt::{self, Write};
use std::io;
use std::ops::Deref;
use std::{fs, path::Path};

struct Context {
    verbose: bool,
    lang: String,
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
#let resultblock(bgcolor: white, stroke: 1pt + luma(150), content) = [
    #move(
        align(
            right, box(
                inset: 0pt, height: 0pt, 
                text(size: 10pt, fill: luma(140))[_Result:_])),
            dx: -4em, dy: 12pt)
    #block(fill: bgcolor, outset: 5pt, radius: 3pt, width: 100%, stroke: stroke, raw(content))
]


"###;

#[derive(Debug, Default)]
enum J2TErrorKind {
    Json(tinyjson::UnexpectedValue),
    Md(String),
    Io(io::Error),
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
impl From<io::Error> for J2TError {
    fn from(s: io::Error) -> J2TError {
        J2TError {
            kind: J2TErrorKind::Io(s),
            ..Default::default()
        }
    }
}

fn markdown_to_typst(n: &Node, out: &mut dyn Write) -> Result<(), J2TError> {
    match n {
        // TODO: implement markdown-to-typst translation
        Node::Root(ref r) => {
            r.children
                .iter()
                .map(|n2| markdown_to_typst(n2, out))
                .for_each(drop);
        }
        Node::InlineCode(ref ic) => {
            write!(out, "`{}`", ic.value).expect("write!()");
        }
        Node::Heading(ref h) => {
            write!(
                out,
                "{} ",
                std::iter::repeat("=")
                    .take(h.depth as usize)
                    .collect::<Vec<&str>>()
                    .join("")
            )
            .expect("write!()");
            h.children
                .iter()
                .map(|n2| markdown_to_typst(n2, out))
                .for_each(drop);
            out.write_str("\n\n").expect("write_str()");
        }
        Node::Paragraph(ref p) => {
            p.children
                .iter()
                .map(|n2| markdown_to_typst(n2, out))
                .for_each(drop);
            out.write_str("\n").expect("write_str()");
        }
        Node::Text(ref t) => {
            out.write_str(t.value.as_str()).expect("write_str()");
        }
        Node::Code(ref c) => {
            write!(
                out,
                "```{}\n",
                c.lang.as_ref().map(String::as_str).unwrap_or("")
            );
            out.write_str(c.value.as_str());
            out.write_str("```\n");
        }
        _ => (),
    }
    Ok(())
}

fn convert_markdown_to_typst(s: &str) -> Result<String, J2TError> {
    let po = markdown::ParseOptions::default();
    let ast = markdown::to_mdast(s, &po)?;
    println!("{:?}", ast);
    let mut s = String::new();
    markdown_to_typst(&ast, &mut s).expect("markdown_to_typst():");
    Ok(s)
}

fn format_cell_result(
    ctx: &Context,
    cell: &HashMap<String, JsonValue>,
) -> Result<String, J2TError> {
    let content = Vec::<JsonValue>::try_from(cell["outputs"].clone())?;

    let get_type = |output: &JsonValue| {
        let o =
            HashMap::<String, JsonValue>::try_from(output.clone()).expect("output object in cell");
        (
            o.get("output_type")
                .map(|oo| String::try_from(oo.clone()).expect("output type is string"))
                .clone(),
            o.get("text").map(|e| e.clone()),
            o.get("data").map(|e| e.clone()),
        )
    };
    let kinds = content.iter().map(get_type).collect::<Vec<_>>();

    // Prefer execute_result - text/plain. Then use stream / stderr.
    let execute_result_str = "execute_result".to_string();
    let stream_str = "stream".to_string();

    // Weird ordering: we want to prefer `execute_result` over `stream`.
    for result_type in &[execute_result_str, stream_str] {
        for k in kinds.iter() {
            if k.0.as_ref() == Some(result_type) {
                // Type `data`
                if let Some(ref data_obj) = k.2 {
                    let data = HashMap::<_, _>::try_from(data_obj.clone())
                        .expect("jsonvalue to hashmap failed for parsing cell output data");
                    if let Some(e) = data.get("text/plain") {
                        return Ok(strip_ansi_codes(join_json_lines_array(e.clone())));
                    } // To do: other MIME types such as text/html and text/latex might typically be
                      // available.
                }

                // Type `stream`
                if let Some(ref text) = k.1 {
                    return Ok(strip_ansi_codes(join_json_lines_array(text.clone())));
                }
            }
        }
    }

    Ok(String::new())
}

fn strip_ansi_codes(s: String) -> String {
    // TODO: implement this functionality.
    s
}

fn join_json_lines_array(lines: JsonValue) -> String {
    Vec::<_>::try_from(lines)
        .expect("could not convert string array to vec of json values")
        .into_iter()
        .map(|s| <JsonValue as TryInto<String>>::try_into(s).unwrap())
        .collect::<Vec<String>>()
        .join("")
}

fn format_cell(ctx: &Context, cell: &JsonValue) -> Result<String, J2TError> {
    let hm: HashMap<_, _> = cell.clone().try_into()?;
    let cell_type = String::try_from(hm["cell_type"].clone()).expect("string from cell_type");

    if cell_type == "markdown" {
        let joined: String =
            <Vec<JsonValue> as TryFrom<JsonValue>>::try_from(hm["source"].clone())?
                .into_iter()
                .map(|s| <JsonValue as TryInto<String>>::try_into(s).unwrap())
                .collect::<Vec<String>>()
                .join("");
        convert_markdown_to_typst(&joined)
    } else if cell_type == "code" {
        let exec_count = f64::try_from(hm["execution_count"].clone()).unwrap();
        let joined_code: String =
            <Vec<JsonValue> as TryFrom<JsonValue>>::try_from(hm["source"].clone())?
                .into_iter()
                .map(|s| <JsonValue as TryInto<String>>::try_into(s).unwrap())
                .collect::<Vec<String>>()
                .join("");
        assert!(
            !joined_code.contains('`'),
            "Currently, code is not allowed to contain backticks!"
        );
        let result_joined = format_cell_result(ctx, &hm)?;
        let code_content = format!(
            r#"
#move(align(right, box(text([[{}]], fill: blue), fill: red, inset: 0pt, height: 0pt)), dx: -25pt, dy: 10pt)
#codeblock(lang: "{}", `{}`.text)
#resultblock(`{}`.text)

"#,
            exec_count, ctx.lang, joined_code, result_joined
        );

        Ok(code_content)
    } else {
        Ok(String::new())
    }
}

fn main() {
    let (args, _rest) = opts! {
        synopsis "Convert a jupyter notebook into typst source code.";
        opt verbose:bool, desc:"Enable verbosity";
        param infile:String, desc:"Input file name";
        param outfile:String, desc:"Input file name";
    }
    .parse_or_exit();

    let parsed_json = parse_notebook_file(&args.infile);
    let parsed_dict = <HashMap<_, _>>::try_from(parsed_json.clone()).unwrap();

    let metadata = HashMap::<_, _>::try_from(parsed_dict["metadata"].clone()).unwrap();
    let kernelspec = HashMap::<_, _>::try_from(metadata["kernelspec"].clone()).unwrap();
    let language: String = kernelspec["language"].clone().try_into().unwrap();

    let ctx = Context {
        verbose: args.verbose,
        lang: language,
    };

    notebook_overview(&ctx, &parsed_json);

    let cells =
        <Vec<JsonValue> as TryFrom<JsonValue>>::try_from(parsed_dict["cells"].clone()).unwrap();

    let mut outfile = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(args.outfile)
        .expect("open output file");
    use std::io::Write;
    outfile.write(document_root.as_bytes());

    let ixs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, cells.len() - 1];

    for i in ixs {
        write!(
            outfile,
            "{}",
            format_cell(&ctx, &cells[i]).expect("format failed")
        );
    }
}
