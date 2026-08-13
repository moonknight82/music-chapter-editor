use std::io::Cursor;

use rand::Rng;
use xmltree::{Element, EmitterConfig, XMLNode};

use crate::models::{ChapterDisplay, ChapterEntry, Edition, ImportedChapters};
use crate::timecode::{format_timestamp_ns, parse_timestamp_ns};

fn direct_elements<'a>(element: &'a Element, name: &str) -> impl Iterator<Item = &'a Element> {
    element.children.iter().filter_map(move |node| match node {
        XMLNode::Element(child) if child.name == name => Some(child),
        _ => None,
    })
}

fn child_text(element: &Element, name: &str) -> Option<String> {
    direct_elements(element, name)
        .next()
        .and_then(Element::get_text)
        .map(|text| text.trim().to_string())
}

fn child_text_alias(element: &Element, current: &str, legacy: &str) -> Option<String> {
    child_text(element, current).or_else(|| child_text(element, legacy))
}

fn bool_child(element: &Element, name: &str) -> bool {
    matches!(
        child_text(element, name).as_deref(),
        Some("1") | Some("true")
    )
}

fn contains_descendant(element: &Element, names: &[&str]) -> bool {
    element.children.iter().any(|node| match node {
        XMLNode::Element(child) => {
            names.contains(&child.name.as_str()) || contains_descendant(child, names)
        }
        _ => false,
    })
}

fn parse_display(element: &Element) -> ChapterDisplay {
    ChapterDisplay {
        title: child_text_alias(element, "ChapterString", "ChapString").unwrap_or_default(),
        language: child_text_alias(element, "ChapterLanguage", "ChapLanguage")
            .unwrap_or_else(|| "und".into()),
        language_ietf: child_text_alias(element, "ChapLanguageIETF", "ChapterLanguageIETF"),
    }
}

fn parse_chapter(element: &Element, source_index: usize) -> Result<ChapterEntry, String> {
    let start_text = child_text(element, "ChapterTimeStart")
        .ok_or_else(|| format!("Chapter {} has no start time", source_index + 1))?;
    let start_ns = parse_timestamp_ns(&start_text)?;
    let end_ns = child_text(element, "ChapterTimeEnd")
        .map(|value| parse_timestamp_ns(&value))
        .transpose()?;
    let displays: Vec<_> = direct_elements(element, "ChapterDisplay")
        .map(parse_display)
        .collect();
    let title = displays
        .first()
        .map(|display| display.title.clone())
        .unwrap_or_default();
    Ok(ChapterEntry {
        uid: child_text(element, "ChapterUID"),
        start_ns,
        end_ns,
        title,
        displays,
        source_index: Some(source_index),
    })
}

pub fn parse_chapter_xml(xml: &str) -> Result<ImportedChapters, String> {
    let root = Element::parse(Cursor::new(xml.as_bytes()))
        .map_err(|error| format!("Invalid Matroska chapter XML: {error}"))?;
    if root.name != "Chapters" {
        return Err("The XML root must be <Chapters>".into());
    }

    let mut advanced_reasons = Vec::new();
    let mut editions = Vec::new();
    for (edition_index, element) in direct_elements(&root, "EditionEntry").enumerate() {
        let ordered = bool_child(element, "EditionFlagOrdered");
        if ordered {
            advanced_reasons.push("ordered editions");
        }
        if direct_elements(element, "ChapterAtom")
            .any(|atom| direct_elements(atom, "ChapterAtom").next().is_some())
        {
            advanced_reasons.push("nested chapters");
        }
        if contains_descendant(
            element,
            &[
                "ChapterSegmentUID",
                "ChapterSegmentEditionUID",
                "ChapterTrack",
            ],
        ) {
            advanced_reasons.push("linked or track-scoped chapters");
        }
        let chapters = direct_elements(element, "ChapterAtom")
            .enumerate()
            .map(|(index, atom)| parse_chapter(atom, index))
            .collect::<Result<Vec<_>, _>>()?;
        editions.push(Edition {
            uid: child_text(element, "EditionUID"),
            hidden: bool_child(element, "EditionFlagHidden"),
            default: bool_child(element, "EditionFlagDefault"),
            ordered,
            chapters,
            source_index: Some(edition_index),
        });
    }

    if editions.is_empty() {
        editions.push(Edition {
            uid: None,
            hidden: false,
            default: true,
            ordered: false,
            chapters: Vec::new(),
            source_index: None,
        });
    }
    advanced_reasons.sort_unstable();
    advanced_reasons.dedup();
    let advanced = !advanced_reasons.is_empty();
    Ok(ImportedChapters {
        editions,
        advanced,
        advanced_reason: advanced.then(|| {
            format!(
                "This file uses {}. Version 0.1 opens it read-only to avoid flattening its structure.",
                advanced_reasons.join(", ")
            )
        }),
        original_xml: xml.to_string(),
    })
}

fn set_text_child(element: &mut Element, name: &str, value: Option<&str>, preferred_index: usize) {
    let existing = element
        .children
        .iter()
        .position(|node| matches!(node, XMLNode::Element(child) if child.name == name));
    match (existing, value) {
        (Some(index), Some(value)) => {
            if let XMLNode::Element(child) = &mut element.children[index] {
                child.children.clear();
                child.children.push(XMLNode::Text(value.to_string()));
            }
        }
        (Some(index), None) => {
            element.children.remove(index);
        }
        (None, Some(value)) => {
            let mut child = Element::new(name);
            child.children.push(XMLNode::Text(value.to_string()));
            element.children.insert(
                preferred_index.min(element.children.len()),
                XMLNode::Element(child),
            );
        }
        (None, None) => {}
    }
}

fn ensure_display(atom: &mut Element, chapter: &ChapterEntry) {
    if let Some(index) = atom
        .children
        .iter()
        .position(|node| matches!(node, XMLNode::Element(child) if child.name == "ChapterDisplay"))
    {
        if let XMLNode::Element(display) = &mut atom.children[index] {
            display.children.retain(|node| {
                !matches!(node, XMLNode::Element(child) if matches!(child.name.as_str(), "ChapString" | "ChapLanguage" | "ChapterLanguageIETF"))
            });
            set_text_child(display, "ChapterString", Some(&chapter.title), 0);
            if !chapter.displays.is_empty() {
                let source = &chapter.displays[0];
                set_text_child(
                    display,
                    "ChapterLanguage",
                    Some(if source.language.is_empty() {
                        "und"
                    } else {
                        &source.language
                    }),
                    1,
                );
                set_text_child(
                    display,
                    "ChapLanguageIETF",
                    source.language_ietf.as_deref(),
                    2,
                );
            }
        }
        return;
    }
    let source_display = chapter.displays.first().cloned().unwrap_or(ChapterDisplay {
        title: chapter.title.clone(),
        language: "und".into(),
        language_ietf: None,
    });
    let mut display = Element::new("ChapterDisplay");
    set_text_child(&mut display, "ChapterString", Some(&chapter.title), 0);
    set_text_child(
        &mut display,
        "ChapterLanguage",
        Some(if source_display.language.is_empty() {
            "und"
        } else {
            &source_display.language
        }),
        1,
    );
    if let Some(ietf) = source_display.language_ietf.as_deref() {
        set_text_child(&mut display, "ChapLanguageIETF", Some(ietf), 2);
    }
    atom.children.push(XMLNode::Element(display));
}

fn fresh_uid() -> String {
    let mut rng = rand::rng();
    loop {
        let value: u64 = rng.random();
        if value != 0 {
            return value.to_string();
        }
    }
}

pub fn ensure_uids(editions: &mut [Edition]) {
    for edition in editions {
        if edition.uid.is_none() {
            edition.uid = Some(fresh_uid());
        }
        for chapter in &mut edition.chapters {
            if chapter.uid.is_none() {
                chapter.uid = Some(fresh_uid());
            }
        }
    }
}

fn render_atom(chapter: &ChapterEntry, original: Option<&Element>) -> Element {
    let mut atom = original
        .cloned()
        .unwrap_or_else(|| Element::new("ChapterAtom"));
    set_text_child(&mut atom, "ChapterUID", chapter.uid.as_deref(), 0);
    set_text_child(
        &mut atom,
        "ChapterTimeStart",
        Some(&format_timestamp_ns(chapter.start_ns)),
        1,
    );
    let end = chapter.end_ns.map(format_timestamp_ns);
    set_text_child(&mut atom, "ChapterTimeEnd", end.as_deref(), 2);
    ensure_display(&mut atom, chapter);
    atom
}

fn render_edition(edition: &Edition, original: Option<&Element>) -> Element {
    let mut output = original
        .cloned()
        .unwrap_or_else(|| Element::new("EditionEntry"));
    let originals: Vec<Element> = original
        .map(|element| direct_elements(element, "ChapterAtom").cloned().collect())
        .unwrap_or_default();
    output
        .children
        .retain(|node| !matches!(node, XMLNode::Element(child) if child.name == "ChapterAtom"));

    set_text_child(&mut output, "EditionUID", edition.uid.as_deref(), 0);
    set_text_child(
        &mut output,
        "EditionFlagHidden",
        Some(if edition.hidden { "1" } else { "0" }),
        1,
    );
    set_text_child(
        &mut output,
        "EditionFlagDefault",
        Some(if edition.default { "1" } else { "0" }),
        2,
    );
    set_text_child(
        &mut output,
        "EditionFlagOrdered",
        Some(if edition.ordered { "1" } else { "0" }),
        3,
    );
    let insertion_index = output.children.len();

    let mut rendered = Vec::new();
    for chapter in &edition.chapters {
        let original_atom = chapter
            .source_index
            .and_then(|index| originals.get(index))
            .or_else(|| {
                chapter.uid.as_ref().and_then(|uid| {
                    originals
                        .iter()
                        .find(|atom| child_text(atom, "ChapterUID").as_ref() == Some(uid))
                })
            });
        rendered.push(XMLNode::Element(render_atom(chapter, original_atom)));
    }
    for (offset, node) in rendered.into_iter().enumerate() {
        output
            .children
            .insert((insertion_index + offset).min(output.children.len()), node);
    }
    output
}

pub fn render_chapter_xml(
    editions: &mut [Edition],
    original_xml: Option<&str>,
) -> Result<String, String> {
    ensure_uids(editions);
    let mut root = match original_xml {
        Some(xml) => Element::parse(Cursor::new(xml.as_bytes()))
            .map_err(|error| format!("Could not reuse the original chapter XML: {error}"))?,
        None => Element::new("Chapters"),
    };
    let originals: Vec<Element> = direct_elements(&root, "EditionEntry").cloned().collect();
    let insertion_index = root
        .children
        .iter()
        .position(|node| matches!(node, XMLNode::Element(child) if child.name == "EditionEntry"))
        .unwrap_or(root.children.len());
    root.children
        .retain(|node| !matches!(node, XMLNode::Element(child) if child.name == "EditionEntry"));
    for (offset, edition) in editions.iter().enumerate() {
        let original = edition
            .source_index
            .and_then(|index| originals.get(index))
            .or_else(|| {
                edition.uid.as_ref().and_then(|uid| {
                    originals
                        .iter()
                        .find(|item| child_text(item, "EditionUID").as_ref() == Some(uid))
                })
            });
        root.children.insert(
            (insertion_index + offset).min(root.children.len()),
            XMLNode::Element(render_edition(edition, original)),
        );
    }
    let mut output = Vec::new();
    root.write_with_config(
        &mut output,
        EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(true),
    )
    .map_err(|error| format!("Could not generate chapter XML: {error}"))?;
    String::from_utf8(output).map_err(|error| error.to_string())
}

pub fn validate_editions(editions: &[Edition], duration_ns: u64) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    for (edition_index, edition) in editions.iter().enumerate() {
        let mut previous = None;
        for (chapter_index, chapter) in edition.chapters.iter().enumerate() {
            if let Some(previous) = previous {
                if chapter.start_ns <= previous {
                    return Err(format!(
                        "Edition {}, chapter {} must start after the previous chapter",
                        edition_index + 1,
                        chapter_index + 1
                    ));
                }
            }
            if duration_ns > 0 && chapter.start_ns > duration_ns {
                return Err(format!(
                    "Edition {}, chapter {} starts after the media ends",
                    edition_index + 1,
                    chapter_index + 1
                ));
            }
            if let Some(end) = chapter.end_ns {
                if end <= chapter.start_ns {
                    return Err(format!(
                        "Edition {}, chapter {} ends before it starts",
                        edition_index + 1,
                        chapter_index + 1
                    ));
                }
            }
            if chapter.title.trim().is_empty() {
                warnings.push(format!(
                    "Edition {}, chapter {} has a blank title",
                    edition_index + 1,
                    chapter_index + 1
                ));
            }
            let next_start = edition
                .chapters
                .get(chapter_index + 1)
                .map(|next| next.start_ns);
            let duration = next_start
                .or(chapter.end_ns)
                .or((duration_ns > 0).then_some(duration_ns));
            if let Some(end) = duration {
                if end > chapter.start_ns && end - chapter.start_ns < 1_000_000_000 {
                    warnings.push(format!(
                        "Edition {}, chapter {} is shorter than one second",
                        edition_index + 1,
                        chapter_index + 1
                    ));
                }
            }
            previous = Some(chapter.start_ns);
        }
    }
    Ok(warnings)
}

pub fn semantically_equal(left: &[Edition], right: &[Edition]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right).all(|(a, b)| {
        a.uid == b.uid
            && a.hidden == b.hidden
            && a.default == b.default
            && a.ordered == b.ordered
            && a.chapters.len() == b.chapters.len()
            && a.chapters.iter().zip(&b.chapters).all(|(x, y)| {
                x.uid == y.uid
                    && x.start_ns == y.start_ns
                    && x.end_ns == y.end_ns
                    && x.title == y.title
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = r#"<?xml version="1.0"?>
<Chapters><EditionEntry><EditionUID>10</EditionUID><EditionFlagDefault>1</EditionFlagDefault>
<ChapterAtom><ChapterUID>20</ChapterUID><ChapterTimeStart>00:00:00.000000000</ChapterTimeStart>
<ChapterDisplay><ChapString>Intro</ChapString><ChapLanguage>eng</ChapLanguage></ChapterDisplay></ChapterAtom>
<ChapterAtom><ChapterUID>21</ChapterUID><ChapterTimeStart>00:02:03.250</ChapterTimeStart>
<ChapterDisplay><ChapString>Song</ChapString><ChapLanguage>eng</ChapLanguage><ChapLanguageIETF>en</ChapLanguageIETF></ChapterDisplay></ChapterAtom>
</EditionEntry></Chapters>"#;

    #[test]
    fn round_trips_and_preserves_languages() {
        let parsed = parse_chapter_xml(SIMPLE).unwrap();
        assert!(!parsed.advanced);
        assert_eq!(parsed.editions[0].chapters[1].start_ns, 123_250_000_000);
        let mut editions = parsed.editions.clone();
        editions[0].chapters[1].title = "New song".into();
        let xml = render_chapter_xml(&mut editions, Some(SIMPLE)).unwrap();
        let reparsed = parse_chapter_xml(&xml).unwrap();
        assert_eq!(reparsed.editions[0].chapters[1].title, "New song");
        assert_eq!(
            reparsed.editions[0].chapters[1].displays[0]
                .language_ietf
                .as_deref(),
            Some("en")
        );
    }

    #[test]
    fn marks_nested_chapters_read_only() {
        let xml = "<Chapters><EditionEntry><ChapterAtom><ChapterTimeStart>00:00:00</ChapterTimeStart><ChapterAtom><ChapterTimeStart>00:00:01</ChapterTimeStart></ChapterAtom></ChapterAtom></EditionEntry></Chapters>";
        assert!(parse_chapter_xml(xml).unwrap().advanced);
    }

    #[test]
    fn supports_empty_and_multiple_flat_editions() {
        let empty = parse_chapter_xml("<Chapters></Chapters>").unwrap();
        assert_eq!(empty.editions.len(), 1);
        assert!(empty.editions[0].default);
        assert!(empty.editions[0].chapters.is_empty());

        let xml = r#"<Chapters>
<EditionEntry><EditionUID>1</EditionUID><EditionFlagDefault>1</EditionFlagDefault>
<ChapterAtom><ChapterUID>11</ChapterUID><ChapterTimeStart>00:00:00</ChapterTimeStart><ChapterTimeEnd>00:01:00</ChapterTimeEnd><ChapterDisplay><ChapterString>Prólogo</ChapterString><ChapterLanguage>por</ChapterLanguage><ChapLanguageIETF>pt-BR</ChapLanguageIETF></ChapterDisplay><ChapterDisplay><ChapterString>Prologue</ChapterString><ChapterLanguage>eng</ChapterLanguage></ChapterDisplay><ChapterFlagEnabled>1</ChapterFlagEnabled></ChapterAtom></EditionEntry>
<EditionEntry><EditionUID>2</EditionUID><ChapterAtom><ChapterUID>21</ChapterUID><ChapterTimeStart>00:00:00</ChapterTimeStart><ChapterDisplay><ChapterString>第二部</ChapterString><ChapterLanguage>jpn</ChapterLanguage></ChapterDisplay></ChapterAtom></EditionEntry>
</Chapters>"#;
        let parsed = parse_chapter_xml(xml).unwrap();
        assert!(!parsed.advanced);
        assert_eq!(parsed.editions.len(), 2);
        assert_eq!(parsed.editions[0].chapters[0].end_ns, Some(60_000_000_000));
        assert_eq!(parsed.editions[0].chapters[0].displays.len(), 2);
        assert_eq!(parsed.editions[1].chapters[0].title, "第二部");

        let mut editions = parsed.editions;
        editions[0].chapters[0].title = "Abertura".into();
        let rendered = render_chapter_xml(&mut editions, Some(xml)).unwrap();
        assert!(rendered.contains("<ChapterFlagEnabled>1</ChapterFlagEnabled>"));
        assert!(rendered.contains("<ChapterString>Prologue</ChapterString>"));
        let reparsed = parse_chapter_xml(&rendered).unwrap();
        assert_eq!(reparsed.editions[0].chapters[0].uid.as_deref(), Some("11"));
        assert_eq!(reparsed.editions[0].chapters[0].title, "Abertura");
        assert_eq!(
            reparsed.editions[0].chapters[0].displays[0]
                .language_ietf
                .as_deref(),
            Some("pt-BR")
        );
    }

    #[test]
    fn marks_ordered_and_linked_chapters_read_only() {
        let xml = r#"<Chapters><EditionEntry><EditionFlagOrdered>1</EditionFlagOrdered>
<ChapterAtom><ChapterTimeStart>00:00:00</ChapterTimeStart><ChapterSegmentUID>001122</ChapterSegmentUID></ChapterAtom>
</EditionEntry></Chapters>"#;
        let parsed = parse_chapter_xml(xml).unwrap();
        assert!(parsed.advanced);
        let reason = parsed.advanced_reason.unwrap();
        assert!(reason.contains("ordered editions"));
        assert!(reason.contains("linked or track-scoped chapters"));
    }

    fn test_chapter(start_ns: u64, end_ns: Option<u64>, title: &str) -> ChapterEntry {
        ChapterEntry {
            uid: None,
            start_ns,
            end_ns,
            title: title.into(),
            displays: Vec::new(),
            source_index: None,
        }
    }

    fn test_edition(chapters: Vec<ChapterEntry>) -> Edition {
        Edition {
            uid: None,
            hidden: false,
            default: true,
            ordered: false,
            chapters,
            source_index: None,
        }
    }

    #[test]
    fn validates_timeline_errors_and_non_blocking_warnings() {
        let invalid_order = test_edition(vec![
            test_chapter(1_000_000_000, None, "One"),
            test_chapter(1_000_000_000, None, "Two"),
        ]);
        assert!(validate_editions(&[invalid_order], 10_000_000_000).is_err());

        let after_media = test_edition(vec![test_chapter(11_000_000_000, None, "Late")]);
        assert!(validate_editions(&[after_media], 10_000_000_000).is_err());

        let invalid_end = test_edition(vec![test_chapter(
            5_000_000_000,
            Some(4_000_000_000),
            "Backwards",
        )]);
        assert!(validate_editions(&[invalid_end], 10_000_000_000).is_err());

        let warnings = validate_editions(
            &[test_edition(vec![
                test_chapter(0, None, ""),
                test_chapter(500_000_000, None, "Short"),
            ])],
            10_000_000_000,
        )
        .unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("blank title"))
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("shorter than one second"))
        );
    }
}
