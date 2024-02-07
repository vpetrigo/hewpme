use std::fmt::{Formatter, Write};
use std::fs;
use std::io::Read;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tinytemplate::TinyTemplate;

use crate::helper::{ChattersList, SafeTwitchEventList};

#[derive(Serialize, Debug)]
struct Content<T>
where
    T: IntoIterator,
{
    chatters_list: T,
    followers_list: T,
    subscribers_list: T,
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

pub(crate) fn run_server(chatters_list: ChattersList, event_list: SafeTwitchEventList) {
    rouille::start_server("0.0.0.0:12345", move |request| {
        let response = rouille::match_assets(request, "public/");

        if response.is_success() {
            return response;
        }

        rouille::router!(request,
            (GET) (/) => {
                if let Ok(page) = generate_credit_page(&chatters_list, &event_list) {
                    return rouille::Response::html(page)
                }

                rouille::Response::empty_404()
            },
            _ => rouille::Response::empty_404()
        )
    });
}

fn generate_credits_text<T: IntoIterator + Serialize>(
    chatters_list: T,
    followers_list: T,
    subscribers_list: T,
) -> Result<String> {
    let template = read_index_template()?;

    add_chatters_to_index_page(chatters_list, followers_list, subscribers_list, template)
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
    followers_list: T,
    subscribers_list: T,
    index_template: String,
) -> Result<String> {
    let mut tt = TinyTemplate::new();
    let context = Content {
        chatters_list,
        followers_list,
        subscribers_list,
    };

    tt.add_template("index", index_template.as_str())?;
    tt.add_formatter("followers", chatter_name_formatter);
    tt.add_formatter("subscribers", chatter_name_formatter);
    tt.add_formatter("chatters", chatter_name_formatter);

    Ok(tt.render("index", &context)?)
}

fn chatter_name_formatter(name: &Value, out: &mut String) -> tinytemplate::error::Result<()> {
    if let Value::String(str) = name {
        out.write_str(str)?;
        out.write_char('\n')?;
    }

    Ok(())
}

fn generate_credit_page(
    chatters_list: &ChattersList,
    event_list: &SafeTwitchEventList,
) -> Result<String> {
    let guard1 = chatters_list.blocking_lock();
    let chatters = guard1.iter().collect::<Vec<_>>();

    let guard2 = event_list.get_followers();
    let followers = guard2.iter().collect::<Vec<_>>();

    let guard3 = event_list.get_subscribers();
    let subs = guard3.iter().collect::<Vec<_>>();

    generate_credits_text(chatters, followers, subs)
}
