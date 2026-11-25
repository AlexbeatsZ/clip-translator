use std::ffi::CStr;

use cocoa::{
    appkit::{
        NSFilenamesPboardType, NSHTMLPboardType, NSPasteboard, NSPasteboardTypeHTML,
        NSPasteboardTypeRTF, NSPasteboardTypeString, NSRTFPboardType, NSURLPboardType,
    },
    base::{id, nil},
    foundation::{NSArray, NSString},
};
use objc::{
    runtime::{objc_autorelease, Class, Sel},
    Message,
};

pub(crate) unsafe fn read_pasteboard_text_best_effort(pasteboard: id) -> Option<String> {
    read_string_for_type(pasteboard, NSPasteboardTypeString)
        .or_else(|| read_rtf_plain_text(pasteboard))
        .or_else(|| read_html_plain_text(pasteboard))
        .or_else(|| read_legacy_file_paths(pasteboard))
        .or_else(|| read_file_url_text(pasteboard))
        .or_else(|| read_url_text(pasteboard))
}

unsafe fn read_string_for_type(pasteboard: id, type_id: id) -> Option<String> {
    non_empty_string(nsstring_to_string(NSPasteboard::stringForType(
        pasteboard, type_id,
    ))?)
}

unsafe fn read_string_for_type_name(pasteboard: id, type_name: &str) -> Option<String> {
    let type_id = NSString::alloc(nil).init_str(type_name);
    read_string_for_type(pasteboard, type_id)
}

unsafe fn read_rtf_plain_text(pasteboard: id) -> Option<String> {
    read_rtf_plain_text_for_type(pasteboard, NSPasteboardTypeRTF)
        .or_else(|| read_rtf_plain_text_for_type(pasteboard, NSRTFPboardType))
}

unsafe fn read_rtf_plain_text_for_type(pasteboard: id, type_id: id) -> Option<String> {
    let data = NSPasteboard::dataForType(pasteboard, type_id);
    if data == nil {
        return None;
    }

    let attributed = attributed_string_from_rtf(data)?;
    let string: id = (&*attributed)
        .send_message(Sel::register("string"), ())
        .ok()?;
    non_empty_string(nsstring_to_string(string)?)
}

unsafe fn attributed_string_from_rtf(data: id) -> Option<id> {
    let attributed_class = Class::get("NSAttributedString")?;
    let allocated: id = attributed_class
        .send_message(Sel::register("alloc"), ())
        .ok()?;
    if allocated == nil {
        return None;
    }

    let attributed: id = (&*allocated)
        .send_message(
            Sel::register("initWithRTF:documentAttributes:"),
            (data, nil),
        )
        .ok()?;
    if attributed == nil {
        return None;
    }

    objc_autorelease(attributed);
    Some(attributed)
}

unsafe fn read_html_plain_text(pasteboard: id) -> Option<String> {
    read_string_for_type(pasteboard, NSPasteboardTypeHTML)
        .or_else(|| read_string_for_type(pasteboard, NSHTMLPboardType))
        .and_then(|html| non_empty_string(html_to_text(&html)))
}

unsafe fn read_legacy_file_paths(pasteboard: id) -> Option<String> {
    let plist = NSPasteboard::propertyListForType(pasteboard, NSFilenamesPboardType);
    if plist == nil {
        return None;
    }

    let count = plist.count();
    if count == 0 {
        return None;
    }

    let mut paths = Vec::new();
    for index in 0..count {
        let value = plist.objectAtIndex(index);
        if let Some(path) = nsstring_to_string(value).and_then(non_empty_string) {
            paths.push(path);
        }
    }

    join_lines(paths)
}

unsafe fn read_file_url_text(pasteboard: id) -> Option<String> {
    read_url_strings_for_types(pasteboard, &["public.file-url"], &[NSURLPboardType]).and_then(
        |urls| {
            let paths = urls
                .into_iter()
                .filter_map(|url| file_url_to_path(&url))
                .collect::<Vec<_>>();
            join_lines(paths)
        },
    )
}

unsafe fn read_url_text(pasteboard: id) -> Option<String> {
    read_url_strings_for_types(pasteboard, &["public.url"], &[NSURLPboardType]).and_then(join_lines)
}

unsafe fn read_url_strings_for_types(
    pasteboard: id,
    named_types: &[&str],
    static_types: &[id],
) -> Option<Vec<String>> {
    for type_id in static_types {
        if let Some(value) = read_string_for_type(pasteboard, *type_id) {
            return Some(split_non_empty_lines(&value));
        }
    }

    for type_name in named_types {
        if let Some(value) = read_string_for_type_name(pasteboard, type_name) {
            return Some(split_non_empty_lines(&value));
        }
    }

    None
}

fn html_to_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    let mut entity = String::new();
    let mut in_entity = false;

    for ch in html.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
                if !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            continue;
        }

        if in_entity {
            entity.push(ch);
            if ch == ';' || entity.len() > 12 {
                text.push_str(&decode_html_entity(&entity));
                entity.clear();
                in_entity = false;
            }
            continue;
        }

        match ch {
            '<' => in_tag = true,
            '&' => {
                entity.push(ch);
                in_entity = true;
            }
            _ => text.push(ch),
        }
    }

    if in_entity {
        text.push_str(&entity);
    }

    normalize_whitespace(&text)
}

fn decode_html_entity(entity: &str) -> String {
    match entity {
        "&amp;" => "&".to_string(),
        "&lt;" => "<".to_string(),
        "&gt;" => ">".to_string(),
        "&quot;" => "\"".to_string(),
        "&#39;" | "&apos;" => "'".to_string(),
        _ => entity.to_string(),
    }
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn split_non_empty_lines(value: &str) -> Vec<String> {
    value
        .lines()
        .filter_map(|line| non_empty_string(line.to_string()))
        .collect()
}

fn join_lines(values: Vec<String>) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join("\n"))
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == value.len() {
        Some(value)
    } else {
        Some(trimmed.to_string())
    }
}

fn file_url_to_path(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }

    Some(percent_decode(path))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push(high * 16 + low);
                index += 3;
                continue;
            }
        }

        output.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

unsafe fn nsstring_to_string(value: id) -> Option<String> {
    if value == nil {
        return None;
    }
    let c_string = value.UTF8String();
    if c_string.is_null() {
        return None;
    }
    CStr::from_ptr(c_string)
        .to_str()
        .ok()
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::{file_url_to_path, html_to_text, read_pasteboard_text_best_effort};
    use cocoa::{
        appkit::{NSFilenamesPboardType, NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF},
        base::{id, nil, NO},
        foundation::{NSArray, NSAutoreleasePool, NSData, NSString},
    };

    #[test]
    fn best_effort_reader_prefers_plain_text() {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let pasteboard = NSPasteboard::pasteboardWithUniqueName(nil);
            pasteboard.clearContents();

            let text = NSString::alloc(nil).init_str("plain text");
            assert_ne!(
                NSPasteboard::setString_forType(
                    pasteboard,
                    text,
                    cocoa::appkit::NSPasteboardTypeString
                ),
                NO
            );

            assert_eq!(
                read_pasteboard_text_best_effort(pasteboard).as_deref(),
                Some("plain text")
            );

            pasteboard.releaseGlobally();
            pool.drain();
        }
    }

    #[test]
    fn best_effort_reader_extracts_rtf_text() {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let pasteboard = NSPasteboard::pasteboardWithUniqueName(nil);
            pasteboard.clearContents();

            let data = nsdata_from_bytes(br"{\rtf1\ansi Hello from RTF}");
            assert_ne!(
                NSPasteboard::setData_forType(pasteboard, data, NSPasteboardTypeRTF),
                NO
            );

            assert_eq!(
                read_pasteboard_text_best_effort(pasteboard).as_deref(),
                Some("Hello from RTF")
            );

            pasteboard.releaseGlobally();
            pool.drain();
        }
    }

    #[test]
    fn best_effort_reader_extracts_html_text() {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let pasteboard = NSPasteboard::pasteboardWithUniqueName(nil);
            pasteboard.clearContents();

            let html = NSString::alloc(nil).init_str("<p>Hello &amp; <b>world</b></p>");
            assert_ne!(
                NSPasteboard::setString_forType(pasteboard, html, NSPasteboardTypeHTML),
                NO
            );

            assert_eq!(
                read_pasteboard_text_best_effort(pasteboard).as_deref(),
                Some("Hello & world")
            );

            pasteboard.releaseGlobally();
            pool.drain();
        }
    }

    #[test]
    fn best_effort_reader_extracts_legacy_file_paths() {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let pasteboard = NSPasteboard::pasteboardWithUniqueName(nil);
            pasteboard.clearContents();

            let first = NSString::alloc(nil).init_str("/tmp/one.txt");
            let second = NSString::alloc(nil).init_str("/tmp/two.txt");
            let files = NSArray::arrayWithObjects(nil, &[first, second]);
            assert_ne!(
                NSPasteboard::setPropertyList_forType(pasteboard, files, NSFilenamesPboardType),
                NO
            );

            assert_eq!(
                read_pasteboard_text_best_effort(pasteboard).as_deref(),
                Some("/tmp/one.txt\n/tmp/two.txt")
            );

            pasteboard.releaseGlobally();
            pool.drain();
        }
    }

    #[test]
    fn html_to_text_strips_tags_and_decodes_common_entities() {
        assert_eq!(
            html_to_text("<div>Tom &amp; Jerry<br><span>&lt;ok&gt;</span></div>"),
            "Tom & Jerry <ok>"
        );
    }

    #[test]
    fn file_url_to_path_decodes_percent_encoded_paths() {
        assert_eq!(
            file_url_to_path("file://localhost/tmp/on%20selected.txt").as_deref(),
            Some("/tmp/on selected.txt")
        );
        assert_eq!(file_url_to_path("https://example.com"), None);
    }

    unsafe fn nsdata_from_bytes(bytes: &[u8]) -> id {
        if bytes.is_empty() {
            NSData::data(nil)
        } else {
            NSData::dataWithBytes_length_(nil, bytes.as_ptr().cast::<c_void>(), bytes.len() as u64)
        }
    }
}
