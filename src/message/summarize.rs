//! Summarize replies, after Postbox's Summarize mode: instead of quoting the
//! one message being answered, which already nests every earlier reply, the
//! reply quotes each message of the conversation once, in its own words.
//!
//! The format is Postbox's own. The plain-text part puts every message at one
//! `>` level under a `Name <mailto:address>` line and a date line, so any client
//! folds it as quoted text. The HTML part wraps one card per message in a
//! single `<blockquote type="cite">` with its border removed, so clients still
//! treat the cards as quoted text.
//!
//! A forward summarizes the same way, as Postbox's forwards did, but each
//! message is headed by labelled From, Date, To, CC and Subject lines instead
//! of a name and a date, and the blockquote carries Postbox's forward class.
//!
//! A message keeps what its author wrote. Its quoted history goes, and so does
//! a signature below a `-- ` line; sign-offs and footers without that line stay,
//! as they did in Postbox, because guessing at them would cut real text.
use chrono::{DateTime, Local};
use serde_json::Value;

type Result<T> = std::result::Result<T, &'static str>;

/// At most this many messages are summarized, the newest kept.
pub const MAX_MESSAGES: usize = 5;
const MAX_BODY: usize = 256 * 1024;
/// At most this many To or CC addresses per message; the composer trims to it.
pub const MAX_PEOPLE: usize = 100;

#[derive(Debug, PartialEq)]
struct Entry {
    name: String,
    email: String,
    when: String,
    body: String,
    to: Vec<(String, String)>,
    cc: Vec<(String, String)>,
    subject: String,
}

/// A forward's To or CC list, `[{name, email}]`; absent is empty.
fn people(v: &Value) -> Result<Vec<(String, String)>> {
    let Some(list) = v.as_array() else {
        return if v.is_null() {
            Ok(Vec::new())
        } else {
            Err("invalid_params")
        };
    };
    if list.len() > MAX_PEOPLE {
        return Err("invalid_params");
    }
    Ok(list
        .iter()
        .map(|p| (one_line(text(&p["name"])), one_line(text(&p["email"]))))
        .filter(|(name, email)| !name.is_empty() || !email.is_empty())
        .collect())
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// Postbox's date line, `10 July 2024 at 10:25 AM`, in the local time zone.
fn when(entry: &Value) -> String {
    DateTime::parse_from_rfc3339(text(&entry["date"]))
        .map(|d| {
            d.with_timezone(&Local)
                .format("%-d %B %Y at %-I:%M %p")
                .to_string()
        })
        .unwrap_or_else(|_| text(&entry["when"]).trim().to_owned())
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `{messages: [{name, email, date, when, body, to, cc, subject}], newestFirst,
/// forward}`, messages in any order. Answers the newest `MAX_MESSAGES`, stripped, in the order to show.
fn entries(summary: &Value) -> Result<Vec<Entry>> {
    let list = summary["messages"].as_array().ok_or("invalid_params")?;
    if list.is_empty() || list.len() > 64 {
        return Err("invalid_params");
    }
    let mut dated: Vec<(String, Entry)> = Vec::new();
    for item in list {
        let body = text(&item["body"]);
        if !item.is_object() || body.len() > MAX_BODY {
            return Err("invalid_params");
        }
        let email = one_line(text(&item["email"]));
        let name = match one_line(text(&item["name"])) {
            name if name.is_empty() => email.clone(),
            name => name,
        };
        let stripped = strip(body);
        if stripped.is_empty() && name.is_empty() {
            continue;
        }
        let sort = DateTime::parse_from_rfc3339(text(&item["date"]))
            .map(|d| d.timestamp_millis().to_string())
            .unwrap_or_default();
        dated.push((
            format!("{sort:0>16}"),
            Entry {
                name,
                email,
                when: one_line(&when(item)),
                body: stripped,
                to: people(&item["to"])?,
                cc: people(&item["cc"])?,
                subject: one_line(text(&item["subject"])),
            },
        ));
    }
    dated.sort_by(|a, b| a.0.cmp(&b.0));
    let keep = dated.len().saturating_sub(MAX_MESSAGES);
    let mut out: Vec<Entry> = dated.into_iter().skip(keep).map(|(_, e)| e).collect();
    if summary["newestFirst"] != false {
        out.reverse();
    }
    Ok(out)
}

fn is_signature(line: &str) -> bool {
    line == "-- " || line.trim_end() == "--"
}

/// Where forwarded or top-posted history begins: Outlook's header block or
/// separator, which quote with no `>` at all.
fn history_starts(lines: &[&str], i: usize) -> bool {
    let t = lines[i].trim();
    let next = |n: usize| {
        lines[i + 1..]
            .iter()
            .map(|l| l.trim().trim_matches('*'))
            .filter(|l| !l.is_empty())
            .take(n)
            .collect::<Vec<_>>()
    };
    if t.starts_with("-----Original Message-----") || t.starts_with("-------- Original Message") {
        return true;
    }
    if t.len() >= 20 && t.chars().all(|c| c == '_') {
        return next(1).first().is_some_and(|l| l.starts_with("From:"));
    }
    let t = t.trim_matches('*');
    t.starts_with("From:")
        && next(3)
            .iter()
            .any(|l| l.starts_with("Sent:") || l.starts_with("Date:"))
}

fn quoted(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

/// An attribution such as `On Thu, 18 Jul 2024 at 11:14, Jane <j@x> wrote:`,
/// which Gmail wraps onto a second line.
fn attribution(lines: &[&str], i: usize) -> Option<usize> {
    let t = lines[i].trim();
    if !t.ends_with("wrote:") && !t.ends_with("a écrit :") && !t.ends_with("schrieb:") {
        return None;
    }
    if t.starts_with("On ") || t.starts_with("Le ") || t.starts_with("Am ") {
        return Some(i);
    }
    let previous = lines[..i].iter().rposition(|l| !l.trim().is_empty())?;
    lines[previous]
        .trim()
        .starts_with("On ")
        .then_some(previous)
}

/// The message as its author wrote it: quoted history and a `-- ` signature
/// removed, blank runs collapsed.
pub fn strip(body: &str) -> String {
    let body = body.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = body.split('\n').collect();
    let end = (0..lines.len())
        .find(|&i| is_signature(lines[i]) || history_starts(&lines, i))
        .unwrap_or(lines.len());
    let lines = &lines[..end];
    let mut drop = vec![false; lines.len()];
    for i in 0..lines.len() {
        if quoted(lines[i]) {
            drop[i] = true;
            continue;
        }
        // An attribution goes with the quote under it, or at the very end.
        if let Some(from) = attribution(lines, i) {
            let rest = lines[i + 1..].iter().find(|l| !l.trim().is_empty());
            if rest.is_none_or(|l| quoted(l)) {
                drop[from..=i].iter_mut().for_each(|d| *d = true);
            }
        }
    }
    let mut out: Vec<&str> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if drop[i] {
            continue;
        }
        // Outlook pads a signature with lines of zero-width spaces.
        let line = line
            .trim_end_matches(|c: char| c.is_whitespace() || c == '\u{200b}' || c == '\u{feff}');
        if line.is_empty() && out.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

fn forward(summary: &Value) -> bool {
    summary["forward"] == true
}

/// `Name <address>`, or whichever of the two there is.
fn person(name: &str, email: &str) -> String {
    if email.is_empty() || name == email {
        if email.is_empty() {
            name.to_owned()
        } else {
            email.to_owned()
        }
    } else if name.is_empty() {
        email.to_owned()
    } else {
        format!("{name} <{email}>")
    }
}

fn list(people: &[(String, String)], one: impl Fn(&str, &str) -> String) -> String {
    people
        .iter()
        .map(|(n, e)| one(n, e))
        .collect::<Vec<_>>()
        .join(", ")
}

// Postbox's forward header, labelled and in Markdown-style bold.
fn forward_header(e: &Entry, lines: &mut Vec<String>) {
    lines.push(format!("> *From:* {}", person(&e.name, &e.email)));
    for (label, value) in [
        ("Date", e.when.clone()),
        ("To", list(&e.to, person)),
        ("CC", list(&e.cc, person)),
        ("Subject", e.subject.clone()),
    ] {
        if !value.is_empty() {
            lines.push(format!("> *{label}:* {value}"));
        }
    }
}

fn plain_entries(entries: &[Entry], forwarding: bool) -> String {
    let mut lines: Vec<String> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            lines.extend([">".into(), ">".into()]);
        }
        if forwarding {
            forward_header(e, &mut lines);
        } else {
            reply_header(e, &mut lines);
        }
        for line in e.body.split('\n') {
            lines.push(if line.is_empty() {
                ">".into()
            } else {
                format!("> {line}")
            });
        }
    }
    lines.join("\n")
}

fn reply_header(e: &Entry, lines: &mut Vec<String>) {
    lines.push(if e.email.is_empty() {
        format!("> {}", e.name)
    } else {
        format!("> {} <mailto:{}>", e.name, e.email)
    });
    if !e.when.is_empty() {
        lines.push(format!("> {}", e.when));
    }
}

/// The plain-text quote for `message.composeText`.
pub fn plain(summary: &Value) -> Result<String> {
    Ok(plain_entries(&entries(summary)?, forward(summary)))
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn lines_html(value: &str) -> String {
    escape(value).replace('\n', "<br>\n")
}

// Postbox's markup and inline styles, as its own messages carried them.
fn card(e: &Entry) -> String {
    let name = if e.email.is_empty() {
        format!(
            "<span style=\"color:#485664;padding-right:6px;font-weight:500;\">{}</span>",
            escape(&e.name)
        )
    } else {
        format!(
            "<a style=\"color:#485664 !important;padding-right:6px;font-weight:500;text-decoration:none !important;\" href=\"mailto:{}\">{}</a>",
            escape(&e.email),
            escape(&e.name)
        )
    };
    format!(
        "<div class=\"__pbConvHr\" style=\"margin:30px 25px 10px 25px;\"><div style=\"width:100%;border-top:2px solid rgba(146,154,163,0.7);padding-top:10px;\"><div style=\"display:inline-block;white-space:nowrap;vertical-align:middle;width:49%;\">{name}</div><div style=\"display:inline-block;white-space:nowrap;vertical-align:middle;width:48%;text-align:right;\"><font color=\"#909AA4\"><span style=\"padding-left:6px\">{}</span></font></div></div></div>\n<div class=\"__pbConvBody\" style=\"color:#909AA4;margin-left:24px;margin-right:24px;\">{}</div>\n",
        escape(&e.when),
        lines_html(&e.body)
    )
}

fn person_html(name: &str, email: &str) -> String {
    if email.is_empty() {
        return escape(name);
    }
    let same = name.is_empty() || name == email;
    let email = escape(email);
    if same {
        format!("<a class=\"moz-txt-link-abbreviated\" href=\"mailto:{email}\">{email}</a>")
    } else {
        format!(
            "{} <a class=\"moz-txt-link-rfc2396E\" href=\"mailto:{email}\">&lt;{email}&gt;</a>",
            escape(name)
        )
    }
}

// Postbox's forward card: the same rule and body, with a labelled header.
fn forward_card(e: &Entry) -> String {
    let mut rest = String::new();
    for (label, value) in [
        ("Date", escape(&e.when)),
        ("To", list(&e.to, person_html)),
        ("CC", list(&e.cc, person_html)),
        ("Subject", escape(&e.subject)),
    ] {
        if !value.is_empty() {
            rest.push_str(&format!("<b>{label}:</b> {value}<br>"));
        }
    }
    format!(
        "<div class=\"__pbConvHr\" style=\"margin:30px 25px 10px 25px;\"><div style=\"display:table;width:100%;border-top:2px solid rgba(146,154,163,0.7);padding-top:10px\"><font color=\"#485664\"><div style=\"display:table-cell;white-space:nowrap;vertical-align:top;width:100%\"><span style=\"padding-right:6px;font-weight:500\"><b>From:</b> {}<br></span><font color=\"#485664\">{rest}</font></div></font></div></div>\n<div class=\"__pbConvBody\" style=\"color:#909AA4;margin-left:24px;margin-right:24px;\">{}</div>\n",
        person_html(&e.name, &e.email),
        lines_html(&e.body)
    )
}

fn cite(in_reply_to: &str) -> String {
    let id = in_reply_to
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    if id.is_empty() || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
        String::new()
    } else {
        format!(" cite=\"mid:{}\"", escape(id))
    }
}

/// The HTML part of an outgoing reply whose body still holds its summary
/// unedited, or `None` when there is no summary or the person changed it, in
/// which case the message goes out as it is written.
pub fn html_body(fields: &Value, rtl: bool) -> Result<Option<String>> {
    let summary = &fields["summary"];
    if !summary.is_object() {
        return Ok(None);
    }
    let entries = entries(summary)?;
    let forwarding = forward(summary);
    let quote = plain_entries(&entries, forwarding);
    let body = text(&fields["body"]).replace("\r\n", "\n");
    let Some(at) = body.find(&quote) else {
        return Ok(None);
    };
    let block = format!(
        "<blockquote type=\"cite\"{} style=\"border: 0px none !important;\"{}>\n{}</blockquote>",
        cite(text(&fields["inReplyTo"])),
        if forwarding {
            " class=\"__pbForwardBlockQuote\""
        } else {
            ""
        },
        entries
            .iter()
            .map(if forwarding { forward_card } else { card })
            .collect::<String>()
    );
    let part = |s: &str| {
        let s = s.trim_end_matches('\n');
        if s.trim().is_empty() {
            String::new()
        } else {
            format!("<div>{}</div>\n", lines_html(s))
        }
    };
    Ok(Some(format!(
        "<html><body{}>{}{}{}</body></html>",
        if rtl { " dir=\"rtl\"" } else { "" },
        part(&body[..at]),
        block,
        part(&body[at + quote.len()..])
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn thread() -> Value {
        json!({"messages": [
            {"name":"Kate Smith","email":"kate@smithco.nz","date":"2010-01-14T22:02:00Z",
             "body":"Hi Ben,\r\n\r\nAny ideas for the south wall?\r\n\r\nThanks,\r\nKate\r\n-- \r\nKate Smith\r\nSmith & Co"},
            {"name":"Kate Smith","email":"kate@smithco.nz","date":"2010-01-14T22:20:00Z",
             "body":"Maple too?\n\nKate\n\nOn 15 Jan 2010, at 9:18 AM, Ben Greene <ben@greenscapes.com> wrote:\n\n> Birch?\n> -- \n> Ben"},
            {"name":"Ben Greene","email":"ben@greenscapes.com","date":"2010-01-14T22:18:00Z",
             "body":"Birch?\n--\nBen Greene"}
        ]})
    }

    #[test]
    fn strips_quotes_and_dash_signatures_but_keeps_sign_offs() {
        assert_eq!(
            strip("Hi Ben,\r\n\r\nThanks,\r\nKate\r\n-- \r\nKate Smith\r\n\r\n> old"),
            "Hi Ben,\n\nThanks,\nKate"
        );
        // Gmail wraps a long attribution; both lines go with the quote.
        assert_eq!(
            strip(
                "Thanks Michael.\n\nOn Thu, 18 Jul 2024 at 10:24, Michael Berias <m@x.au>\nwrote:\n\n> Hi Leighton,\n>\n> More"
            ),
            "Thanks Michael."
        );
        // Outlook top-posts its history with no `>`.
        assert_eq!(
            strip(
                "Hi Leighton,\n\nThank you\n________________________________\nFrom: Leighton\nSent: Thursday\nSubject: Re: x\n\nEarlier text"
            ),
            "Hi Leighton,\n\nThank you"
        );
        // Interleaved answers stay; only the quoted lines go.
        assert_eq!(strip("> Can you?\nYes.\n> And?\nNo."), "Yes.\nNo.");
        // A line that merely ends in "wrote:" is not an attribution unless a quote follows.
        assert_eq!(
            strip("Here is what she wrote:\nhello"),
            "Here is what she wrote:\nhello"
        );
        assert_eq!(
            strip("Kept\n\nSent from my iPhone"),
            "Kept\n\nSent from my iPhone"
        );
        assert_eq!(
            strip("Thank you\r\n\r\n\u{200b}\r\n\u{200b}Best,\r\nM"),
            "Thank you\n\n\u{200b}Best,\nM"
        );
    }

    #[test]
    fn plain_is_one_flat_quote_newest_first() {
        let out = plain(&thread()).unwrap();
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines[0], "> Kate Smith <mailto:kate@smithco.nz>");
        assert_eq!(lines[2], "> Maple too?");
        assert!(out.contains(">\n>\n> Ben Greene <mailto:ben@greenscapes.com>\n"));
        assert!(out.ends_with("> Thanks,\n> Kate"));
        // Every line is quoted, and nothing quoted is quoted twice.
        assert!(
            lines
                .iter()
                .all(|l| l.starts_with('>') && !l.starts_with("> >"))
        );
        assert!(!out.contains("Smith & Co") && !out.contains("Birch?\n> --"));
        let mut oldest = thread();
        oldest["newestFirst"] = json!(false);
        assert!(plain(&oldest).unwrap().ends_with("> Maple too?\n>\n> Kate"));
    }

    #[test]
    fn keeps_only_the_newest_five() {
        let messages: Vec<_> = (0..7)
            .map(|i| json!({"name":format!("P{i}"),"email":"p@x.org","date":format!("2024-01-0{}T00:00:00Z", i + 1),"body":format!("m{i}")}))
            .collect();
        let out = plain(&json!({ "messages": messages })).unwrap();
        assert!(out.starts_with("> P6 ") && out.contains("> m2") && !out.contains("> m1"));
    }

    #[test]
    fn html_needs_the_summary_unedited() {
        let quote = plain(&thread()).unwrap();
        let mut fields = json!({"body": format!("Sounds <good>\n\n{quote}"), "summary": thread(), "inReplyTo":"<a1@x.org>"});
        let html = html_body(&fields, false).unwrap().unwrap();
        assert!(html.starts_with("<html><body><div>Sounds &lt;good&gt;</div>\n<blockquote type=\"cite\" cite=\"mid:a1@x.org\""));
        assert_eq!(html.matches("__pbConvHr").count(), 3);
        assert!(
            !html.contains("Smith &amp; Co") && html.contains("href=\"mailto:kate@smithco.nz\"")
        );
        fields["body"] = json!(format!("Sounds good\n\n{}", quote.replace("Maple", "Oak")));
        assert_eq!(html_body(&fields, false).unwrap(), None);
        assert_eq!(html_body(&json!({"body":"x"}), false).unwrap(), None);
    }

    #[test]
    fn a_placed_summary_sends_as_text_and_cards() {
        use base64::Engine;
        let placed = crate::message::content::request(
            "message.composeText",
            &json!({"signature":"-- \nBen","summarize":thread(),"summary":{"subject":"Trees"}}),
        )
        .unwrap();
        assert_eq!(placed["replySubject"], "Re: Trees");
        let body = format!("Sounds good{}", placed["body"].as_str().unwrap());
        let sent =
            crate::message::compose::build(&json!({"to":"kate@smithco.nz","subject":"Re: Trees",
            "body":body,"summary":thread(),"inReplyTo":"<a1@x.org>","boundary":"b"}))
            .unwrap();
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(sent["raw"].as_str().unwrap())
            .unwrap();
        let raw = String::from_utf8(raw).unwrap();
        assert!(raw.contains("multipart/alternative") && raw.contains("Content-Type: text/html"));
        // Without the summary the same body goes out as plain text only.
        let plain =
            crate::message::compose::build(&json!({"to":"k@x.org","body":body,"boundary":"b"}))
                .unwrap();
        let plain = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(plain["raw"].as_str().unwrap())
            .unwrap();
        assert!(!String::from_utf8(plain).unwrap().contains("text/html"));
    }

    fn forwarded() -> Value {
        json!({"forward": true, "messages": [
            {"name":"dma intern","email":"intern@dma.nz","date":"2024-07-09T22:25:00Z","when":"x",
             "to":[{"name":"Benek Lisefski","email":"hello@benek.nz"}],"subject":"Website",
             "body":"Also are we able?\n\nThanks,\nEmma"},
            {"name":"Benek Lisefski","email":"hello@benek.nz","date":"2024-06-26T21:45:00Z",
             "to":[{"name":"","email":"intern@dma.nz"}],
             "cc":[{"name":"Leighton Hughes","email":"hello@leightonhughes.co.nz"},{"name":"Ann & Co","email":"a@x.nz"}],
             "subject":"Re: Website","body":"Hi Emma,\n-- \nBenek"}
        ]})
    }

    #[test]
    fn a_forward_heads_each_message_with_labelled_lines() {
        let out = plain(&forwarded()).unwrap();
        let when = |d: &str| when(&json!({ "date": d }));
        assert_eq!(
            out,
            format!(
                "> *From:* dma intern <intern@dma.nz>\n> *Date:* {}\n> *To:* Benek Lisefski <hello@benek.nz>\n> *Subject:* Website\n> Also are we able?\n>\n> Thanks,\n> Emma\n>\n>\n\
                 > *From:* Benek Lisefski <hello@benek.nz>\n> *Date:* {}\n> *To:* intern@dma.nz\n> *CC:* Leighton Hughes <hello@leightonhughes.co.nz>, Ann & Co <a@x.nz>\n> *Subject:* Re: Website\n> Hi Emma,",
                when("2024-07-09T22:25:00Z"),
                when("2024-06-26T21:45:00Z")
            )
        );
        // The same messages as a reply keep the reply header.
        let mut reply = forwarded();
        reply["forward"] = json!(false);
        assert!(plain(&reply).unwrap().starts_with("> dma intern <mailto:intern@dma.nz>\n"));
        // Too many addresses, or a list that is not one, is refused.
        let mut many = forwarded();
        many["messages"][0]["cc"] = json!(vec![json!({"email":"a@x.nz"}); MAX_PEOPLE + 1]);
        assert!(plain(&many).is_err());
        many["messages"][0]["cc"] = json!("a@x.nz");
        assert!(plain(&many).is_err());
    }

    #[test]
    fn a_forward_sends_postbox_forward_cards() {
        let quote = plain(&forwarded()).unwrap();
        let fields = json!({"body": format!("Please see this.\n\n{quote}"), "summary": forwarded(), "inReplyTo": ""});
        let html = html_body(&fields, false).unwrap().unwrap();
        assert!(html.contains("<blockquote type=\"cite\" style=\"border: 0px none !important;\" class=\"__pbForwardBlockQuote\">"));
        assert_eq!(html.matches("class=\"__pbConvHr\"").count(), 2);
        assert!(html.contains("<span style=\"padding-right:6px;font-weight:500\"><b>From:</b> dma intern <a class=\"moz-txt-link-rfc2396E\" href=\"mailto:intern@dma.nz\">&lt;intern@dma.nz&gt;</a><br></span><font color=\"#485664\"><b>Date:</b> "));
        assert!(html.contains("<b>To:</b> Benek Lisefski <a class=\"moz-txt-link-rfc2396E\" href=\"mailto:hello@benek.nz\">&lt;hello@benek.nz&gt;</a><br><b>Subject:</b> Website<br></font>"));
        assert!(html.contains("<b>To:</b> <a class=\"moz-txt-link-abbreviated\" href=\"mailto:intern@dma.nz\">intern@dma.nz</a><br><b>CC:</b> Leighton Hughes <a class=\"moz-txt-link-rfc2396E\" href=\"mailto:hello@leightonhughes.co.nz\">&lt;hello@leightonhughes.co.nz&gt;</a>, Ann &amp; Co <a"));
        // CC appears only where a message had one, and the signature is gone.
        assert_eq!(html.matches("<b>CC:</b>").count(), 1);
        assert!(!html.contains("Benek</div>") && html.contains(">Hi Emma,</div>"));
        // An edited forward goes out as written.
        let edited = json!({"body": quote.replace("Emma", "Em"), "summary": forwarded()});
        assert_eq!(html_body(&edited, false).unwrap(), None);
        // Its attachments still go with it, beside the text and the cards.
        use base64::Engine;
        let sent = crate::message::compose::build(&json!({"to":"l@x.nz","subject":"Fwd: Website",
            "body":fields["body"],"summary":forwarded(),"boundary":"b",
            "attachments":[{"filename":"plan.pdf","mimeType":"application/pdf","data":"AP_-AQ"}]}))
        .unwrap();
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(sent["raw"].as_str().unwrap())
            .unwrap();
        let raw = String::from_utf8(raw).unwrap();
        assert!(raw.contains("multipart/mixed") && raw.contains("multipart/alternative"));
        assert!(raw.contains("Content-Type: text/html") && raw.contains("Content-Disposition: attachment; filename=") && raw.contains("plan.pdf"));
        assert!(!raw.contains("In-Reply-To"));
    }

    #[test]
    fn sender_text_cannot_break_out_of_the_markup() {
        let fields = json!({"body": "", "inReplyTo": "<x\" onload=\"y@z>", "summary": {"messages":[
            {"name":"<script>","email":"a\"@x.org","body":"</blockquote><img src=x>"}]}});
        let quote = plain(&fields["summary"]).unwrap();
        let mut fields = fields;
        fields["body"] = json!(quote);
        let html = html_body(&fields, false).unwrap().unwrap();
        assert!(!html.contains("<script>") && !html.contains("<img") && !html.contains("a\"@"));
        assert_eq!(html.matches("</blockquote>").count(), 1);
        assert!(!html.contains("onload=\""));
    }
}
