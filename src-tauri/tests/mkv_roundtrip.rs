use std::fs::{self, File};
use std::io::Write;

use music_chapter_editor_lib::chapter_xml::{parse_chapter_xml, render_chapter_xml};
use music_chapter_editor_lib::media::open_document;
use music_chapter_editor_lib::tools::locate_tools;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn write_test_wav(path: &std::path::Path) {
    let sample_rate = 8_000u32;
    let sample_count = sample_rate * 2;
    let data_size = sample_count * 2;
    let mut file = File::create(path).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_size).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&(sample_rate * 2).to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();
    for index in 0..sample_count {
        let value = (((index as f32 / sample_rate as f32) * 440.0 * std::f32::consts::TAU).sin()
            * 4_000.0) as i16;
        file.write_all(&value.to_le_bytes()).unwrap();
    }
}

fn digest(path: &std::path::Path) -> String {
    hex::encode(Sha256::digest(fs::read(path).unwrap()))
}

#[test]
fn chapter_edit_preserves_media_track_bytes_when_mkvtoolnix_is_available() {
    let Ok(tools) = locate_tools(None) else {
        eprintln!("MKVToolNix is not installed; skipping optional integration fixture");
        return;
    };
    let directory = tempdir().unwrap();
    let wav = directory.path().join("tone.wav");
    let xml = directory.path().join("chapters.xml");
    let mkv = directory.path().join("fixture.mkv");
    let before = directory.path().join("before.wav");
    let after = directory.path().join("after.wav");
    write_test_wav(&wav);
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8"?><Chapters><EditionEntry><EditionUID>100</EditionUID><EditionFlagDefault>1</EditionFlagDefault><ChapterAtom><ChapterUID>101</ChapterUID><ChapterTimeStart>00:00:00.000</ChapterTimeStart><ChapterDisplay><ChapterString>Intro</ChapterString><ChapterLanguage>eng</ChapterLanguage></ChapterDisplay></ChapterAtom><ChapterAtom><ChapterUID>102</ChapterUID><ChapterTimeStart>00:00:01.000</ChapterTimeStart><ChapterDisplay><ChapterString>Song</ChapterString><ChapterLanguage>eng</ChapterLanguage></ChapterDisplay></ChapterAtom></EditionEntry></Chapters>"#,
    )
    .unwrap();
    let merged = tools
        .run(
            "mkvmerge",
            [
                "-o".as_ref(),
                mkv.as_os_str(),
                "--chapters".as_ref(),
                xml.as_os_str(),
                wav.as_os_str(),
            ],
        )
        .unwrap();
    assert!(
        merged.status.success(),
        "{}{}",
        String::from_utf8_lossy(&merged.stdout),
        String::from_utf8_lossy(&merged.stderr)
    );
    assert!(
        tools
            .run(
                "mkvextract",
                [
                    mkv.as_os_str(),
                    "tracks".as_ref(),
                    format!("0:{}", before.display()).as_ref()
                ]
            )
            .unwrap()
            .status
            .success()
    );

    let document = open_document(&mkv, &tools).unwrap();
    let mut editions = document.editions;
    editions[0].chapters[1].title = "Edited song".into();
    let rendered = render_chapter_xml(&mut editions, document.original_xml.as_deref()).unwrap();
    fs::write(&xml, rendered).unwrap();
    let edited = tools
        .run(
            "mkvpropedit",
            [mkv.as_os_str(), "--chapters".as_ref(), xml.as_os_str()],
        )
        .unwrap();
    assert!(
        edited.status.success(),
        "{}{}\n{}",
        String::from_utf8_lossy(&edited.stdout),
        String::from_utf8_lossy(&edited.stderr),
        fs::read_to_string(&xml).unwrap()
    );
    assert!(
        tools
            .run(
                "mkvextract",
                [
                    mkv.as_os_str(),
                    "tracks".as_ref(),
                    format!("0:{}", after.display()).as_ref()
                ]
            )
            .unwrap()
            .status
            .success()
    );
    assert_eq!(digest(&before), digest(&after));
    let verified = open_document(&mkv, &tools).unwrap();
    assert_eq!(verified.editions[0].chapters[1].title, "Edited song");
    assert!(
        !parse_chapter_xml(verified.original_xml.as_deref().unwrap())
            .unwrap()
            .advanced
    );
}
