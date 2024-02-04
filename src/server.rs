use std::fmt::{Formatter, Write};
use std::fs;
use std::io::Read;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tinytemplate::TinyTemplate;

use crate::helper::ChattersList;

#[derive(Serialize, Debug)]
struct Content<T>
where
    T: IntoIterator,
{
    chatters_list: T,
}

#[derive(Debug)]
struct ServerError {
    kind: String,
    message: String,
}

type Result<T> = std::result::Result<T, ServerError>;

impl core::fmt::Display for ServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "kind: {}, message: {}", self.kind, self.message)
    }
}

impl From<std::io::Error> for ServerError {
    fn from(value: std::io::Error) -> Self {
        ServerError {
            kind: String::from("io"),
            message: value.to_string(),
        }
    }
}

impl From<tinytemplate::error::Error> for ServerError {
    fn from(value: tinytemplate::error::Error) -> Self {
        ServerError {
            kind: String::from("template"),
            message: value.to_string(),
        }
    }
}

pub(crate) fn run_server(chatters_list: ChattersList) {
    rouille::start_server("0.0.0.0:12345", move |request| {
        let response = rouille::match_assets(request, "public/");

        if response.is_success() {
            return response;
        }

        rouille::router!(request,
            (GET) (/) => {
                if let Ok(page) = generate_chatters_list_text(&chatters_list) {
                    return rouille::Response::html(page)
                }

                rouille::Response::empty_404()
            },
            _ => rouille::Response::empty_404()
        )
    });
}

fn generate_chatters_list_text(chatters_list: &ChattersList) -> Result<String> {
    let template = read_index_template()?;
    let guard = chatters_list.blocking_lock();
    let chatters: Vec<_> = guard.iter().collect();

    add_chatters_to_index_page(chatters, template)
}

fn read_index_template() -> Result<String> {
    let file_path = Path::new("./public/index.template.html");
    let mut file = fs::File::open(file_path)?;
    let mut buffer = String::new();

    file.read_to_string(&mut buffer)?;

    Ok(buffer)
}

fn add_chatters_to_index_page<T: IntoIterator + Serialize>(
    chatters_list: T,
    index_template: String,
) -> Result<String> {
    let mut tt = TinyTemplate::new();
    let context = Content { chatters_list };

    tt.add_template("index", index_template.as_str())?;
    tt.add_formatter("index", chatter_name_formatter);

    Ok(tt.render("index", &context)?)
}

fn chatter_name_formatter(name: &Value, out: &mut String) -> tinytemplate::error::Result<()> {
    if let Value::String(str) = name {
        out.write_str(str)?;
        out.write_char('\n')?;
    }

    Ok(())
}
