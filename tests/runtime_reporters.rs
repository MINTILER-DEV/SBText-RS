use sbtext_rs_core::compile_source_to_sb3_bytes;
use std::io::Read;

#[test]
fn sound_selectors_preserve_reporters_and_literal_menus() {
    let source = r#"
stage
  var track = "music"
  list tracks = ["music"]
  when flag clicked
    start sound (track)
    play sound (item (1) of [tracks]) until done
    start sound (join ("track-") with (track))
    start sound ("literal track")
  end
end
"#;
    let bytes =
        compile_source_to_sb3_bytes(source, std::path::Path::new("."), Default::default()).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name("project.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let project: serde_json::Value = serde_json::from_str(&text).unwrap();
    let blocks = project["targets"][0]["blocks"].as_object().unwrap();
    let sounds: Vec<_> = blocks
        .values()
        .filter(|b| {
            matches!(
                b["opcode"].as_str(),
                Some("sound_play" | "sound_playuntildone")
            )
        })
        .collect();
    assert_eq!(sounds.len(), 4);
    let mut reporters = Vec::new();
    for sound in sounds {
        let input = &sound["inputs"]["SOUND_MENU"];
        if let Some(id) = input[1].as_str() {
            reporters.push(blocks[id]["opcode"].as_str().unwrap());
        } else {
            assert_eq!(input[1][0], 12);
        }
    }
    assert!(reporters.contains(&"data_itemoflist"));
    assert!(reporters.contains(&"operator_join"));
    assert!(reporters.contains(&"sound_sounds_menu"));
}

#[test]
fn documented_exponential_math_lexes_and_emits_scratch_mathops() {
    let source = "stage\nvar result\nwhen flag clicked\nset [result] to (e ^ (-0.08))\nset [result] to (10 ^ (2))\nend\nend\n";
    let bytes = compile_source_to_sb3_bytes(source, std::path::Path::new("."), false).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name("project.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let p: serde_json::Value = serde_json::from_str(&text).unwrap();
    let ops: Vec<_> = p["targets"][0]["blocks"]
        .as_object()
        .unwrap()
        .values()
        .filter(|b| b["opcode"] == "operator_mathop")
        .map(|b| b["fields"]["OPERATOR"][0].as_str().unwrap())
        .collect();
    assert!(ops.contains(&"e ^"));
    assert!(ops.contains(&"10 ^"));
}

#[test]
fn key_selectors_preserve_list_reporters_and_numeric_keycodes() {
    let source = r#"
stage
  var result
  list keys = ["enter"]
  when flag clicked
    set [result] to (key (item (1) of [keys]) pressed?)
    set [result] to (key (65) pressed?)
    set [result] to (key ("space") pressed?)
  end
end
"#;
    let bytes = compile_source_to_sb3_bytes(source, std::path::Path::new("."), false).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name("project.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let p: serde_json::Value = serde_json::from_str(&text).unwrap();
    let blocks = p["targets"][0]["blocks"].as_object().unwrap();
    let inputs: Vec<_> = blocks
        .values()
        .filter(|b| b["opcode"] == "sensing_keypressed")
        .map(|b| &b["inputs"]["KEY_OPTION"])
        .collect();
    assert_eq!(inputs.len(), 3);
    assert!(inputs.iter().any(|i| i[1]
        .as_str()
        .is_some_and(|id| blocks[id]["opcode"] == "data_itemoflist")));
    assert!(inputs.iter().any(|i| i[1][0] == 4 && i[1][1] == "65"));
    assert!(inputs.iter().any(|i| i[1]
        .as_str()
        .is_some_and(|id| blocks[id]["opcode"] == "sensing_keyoptions")));
}
