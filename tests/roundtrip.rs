use sbtext_rs_core::{
    compile_entry_to_sb3_bytes,
    decompile::decompile_sb3,
    sb3::{self, Sb3Archive},
};
use serde_json::{json, Map};
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> Sb3Archive {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="360"><rect width="480" height="360" fill="#ff0000"/></svg>"##.to_vec();
    let wav = [
        b"RIFF".as_slice(),
        &40u32.to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
        &1u16.to_le_bytes(),
        &1u16.to_le_bytes(),
        &8000u32.to_le_bytes(),
        &16000u32.to_le_bytes(),
        &2u16.to_le_bytes(),
        &16u16.to_le_bytes(),
        b"data",
        &4u32.to_le_bytes(),
        &[0, 0, 0, 0],
    ]
    .concat();
    let image = format!("{:x}.svg", md5::compute(&svg));
    let sound = format!("{:x}.wav", md5::compute(&wav));
    let mut blocks = Map::new();
    let reporters = vec![
        json!({"opcode":"motion_xposition"}),
        json!({"opcode":"motion_yposition"}),
        json!({"opcode":"motion_direction"}),
        json!({"opcode":"looks_size"}),
        json!({"opcode":"sensing_mousedown"}),
        json!({"opcode":"sensing_username"}),
        json!({"opcode":"sensing_loudness"}),
        json!({"opcode":"sensing_dayssince2000"}),
        json!({"opcode":"sound_volume"}),
        json!({"opcode":"looks_costumenumbername","fields":{"NUMBER_NAME":["name",null]}}),
        json!({"opcode":"looks_costumenumbername","fields":{"NUMBER_NAME":["number",null]}}),
        json!({"opcode":"looks_backdropnumbername","fields":{"NUMBER_NAME":["name",null]}}),
        json!({"opcode":"sensing_distanceto","inputs":{"DISTANCETOMENU":[1,"distance-menu"]}}),
        json!({"opcode":"data_itemnumoflist","fields":{"LIST":["items","items"]},"inputs":{"ITEM":[1,[10,"two"]]}}),
        json!({"opcode":"operator_contains","inputs":{"STRING1":[1,[10,"abc"]],"STRING2":[1,[10,"b"]]}}),
        json!({"opcode":"operator_letter_of","inputs":{"LETTER":[1,[4,"2"]],"STRING":[1,[10,"abc"]]}}),
        json!({"opcode":"data_variable","fields":{"VARIABLE":["volume","volume"]}}),
    ];
    for (i, mut reporter) in reporters.into_iter().enumerate() {
        reporter["parent"] = json!(format!("save{i}"));
        blocks.insert(format!("reporter{i}"), reporter);
        blocks.insert(
            format!("save{i}"),
            json!({"opcode":"data_setvariableto","next":format!("save{}",i+1),
            "fields":{"VARIABLE":["out","out"]},"inputs":{"VALUE":[2,format!("reporter{i}")]}}),
        );
    }
    blocks.insert(
        "save17".into(),
        json!({"opcode":"sound_playuntildone","inputs":{"SOUND_MENU":[1,"sound-menu"]}}),
    );
    blocks.insert(
        "flag".into(),
        json!({"opcode":"event_whenflagclicked","topLevel":true,"next":"save0"}),
    );
    blocks.insert(
        "clone".into(),
        json!({"opcode":"control_start_as_clone","topLevel":true,"next":"clone-body"}),
    );
    blocks.insert(
        "clone-body".into(),
        json!({"opcode":"motion_movesteps","inputs":{"STEPS":[1,[4,"3"]]},"next":"rgb"}),
    );
    blocks.insert(
        "rgb".into(),
        json!({"opcode":"pen_setPenColorToColor","inputs":{"COLOR":[1,[9,"#ff8000"]]}}),
    );
    blocks.insert("distance-menu".into(), json!({"opcode":"sensing_distancetomenu","fields":{"DISTANCETOMENU":["_mouse_",null]},"shadow":true}));
    blocks.insert("sound-menu".into(), json!({"opcode":"sound_sounds_menu","fields":{"SOUND_MENU":["door open",null]},"shadow":true}));
    Sb3Archive::new(
        json!({"targets":[
        {"name":"Stage","isStage":true,"blocks":{},"variables":{"out":["out",0],"volume":["volume",23]},"lists":{"items":["items",["one","two"]]},"costumes":[],"sounds":[]},
        {"name":"Player","isStage":false,"blocks":blocks,"variables":{},"lists":{},
         "currentCostume":1,"visible":false,"x":12.5,"y":-14,"direction":45,"size":150,"rotationStyle":"left-right","volume":37,"layerOrder":5,
         "costumes":[
            {"name":"bg","md5ext":image,"rotationCenterX":123.5,"rotationCenterY":82,"bitmapResolution":2},
            {"name":" bg2 \n\"星\" ","md5ext":image,"rotationCenterX":240,"rotationCenterY":180,"bitmapResolution":1}],
         "sounds":[{"name":"door open","md5ext":sound,"rate":8000,"sampleCount":2,"format":""}]}
    ],"extensions":[],"monitors":[],"meta":{}}),
        BTreeMap::from([(image, svg), (sound, wav)]),
    )
}

#[test]
fn single_and_split_roundtrips_preserve_assets_names_geometry_state_and_reporters() {
    for split in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let original = fixture();
        let input = temp.path().join("original.sb3");
        sb3::write_sb3_file(&input, &original).unwrap();
        let output = temp
            .path()
            .join(if split { "split" } else { "game.sbtext" });
        decompile_sb3(&input, Some(&output), split).unwrap();
        let entry = if split {
            output.join("main.sbtext")
        } else {
            output
        };
        // Even default SVG scaling must leave imported Scratch bytes alone.
        let bytes = compile_entry_to_sb3_bytes(&entry, true).unwrap();
        let rebuilt = sb3::read_sb3_bytes(&bytes).unwrap();
        let source = &original.project["targets"][1];
        let target = &rebuilt.project["targets"][1];
        for key in [
            "currentCostume",
            "visible",
            "x",
            "y",
            "direction",
            "size",
            "rotationStyle",
            "volume",
            "layerOrder",
        ] {
            assert_eq!(source[key], target[key], "{key}, split={split}");
        }
        for key in ["costumes", "sounds"] {
            for (a, b) in source[key]
                .as_array()
                .unwrap()
                .iter()
                .zip(target[key].as_array().unwrap())
            {
                for (property, value) in a.as_object().unwrap() {
                    assert_eq!(value, &b[property], "{property}");
                }
            }
            assert_eq!(
                source[key].as_array().unwrap().len(),
                target[key].as_array().unwrap().len()
            );
        }
        for (name, bytes) in &original.assets {
            assert_eq!(Some(bytes), rebuilt.assets.get(name));
        }
        let before: BTreeSet<_> = source["blocks"]
            .as_object()
            .unwrap()
            .values()
            .filter_map(|b| b["opcode"].as_str())
            .collect();
        let after: BTreeSet<_> = target["blocks"]
            .as_object()
            .unwrap()
            .values()
            .filter_map(|b| b["opcode"].as_str())
            .collect();
        assert_eq!(before, after);
        assert!(target["blocks"]
            .as_object()
            .unwrap()
            .values()
            .any(|b| b["opcode"] == "looks_costumenumbername"
                && b["fields"]["NUMBER_NAME"][0] == "name"));
    }
}

#[test]
fn unsupported_reporters_fail_before_writing_lossy_source() {
    let temp = tempfile::tempdir().unwrap();
    let mut p = fixture();
    p.project["targets"][1]["blocks"]["reporter0"]["opcode"] = json!("extension_unknown_reporter");
    let input = temp.path().join("original.sb3");
    let output = temp.path().join("output.sbtext");
    sb3::write_sb3_file(&input, &p).unwrap();
    let error = format!(
        "{:#}",
        decompile_sb3(&input, Some(&output), false).unwrap_err()
    );
    assert!(
        error.contains("Player")
            && error.contains("extension_unknown_reporter")
            && error.contains("reporter0")
    );
    assert!(!output.exists());
}

#[test]
fn missing_sound_assets_fail_instead_of_disappearing() {
    let temp = tempfile::tempdir().unwrap();
    let mut p = fixture();
    p.assets.retain(|name, _| !name.ends_with(".wav"));
    let input = temp.path().join("original.sb3");
    sb3::write_sb3_file(&input, &p).unwrap();
    let error = decompile_sb3(&input, Some(&temp.path().join("out.sbtext")), false).unwrap_err();
    assert!(error.to_string().contains("missing from the archive"));
    assert!(!temp.path().join("out.sbtext").exists());
}

#[test]
fn boolean_procedure_parameters_keep_their_type_and_matching_call_shape() {
    let temp = tempfile::tempdir().unwrap();
    let mut p = fixture();
    p.project["targets"][1]["blocks"] = json!({
        "flag":{"opcode":"event_whenflagclicked","topLevel":true,"next":"call"},
        "call":{"opcode":"procedures_call","mutation":{"proccode":"move %b %s","argumentids":"[\"b\",\"n\"]"},"inputs":{"b":[2,"condition"],"n":[1,[4,"5"]]}},
        "condition":{"opcode":"operator_gt","inputs":{"OPERAND1":[1,[4,"2"]],"OPERAND2":[1,[4,"1"]]}},
        "def":{"opcode":"procedures_definition","topLevel":true,"inputs":{"custom_block":[1,"proto"]},"next":"if"},
        "proto":{"opcode":"procedures_prototype","mutation":{"proccode":"move %b %s","argumentids":"[\"b\",\"n\"]","argumentnames":"[\"hit\",\"amount\"]","argumentdefaults":"[false,\"\"]","warp":"true"}},
        "if":{"opcode":"control_if","inputs":{"CONDITION":[2,"hit"],"SUBSTACK":[2,"move"]}},
        "hit":{"opcode":"argument_reporter_boolean","fields":{"VALUE":["hit",null]}},
        "move":{"opcode":"motion_movesteps","inputs":{"STEPS":[2,"amount"]}},
        "amount":{"opcode":"argument_reporter_string_number","fields":{"VALUE":["amount",null]}}
    });
    let input = temp.path().join("original.sb3");
    let output = temp.path().join("game.sbtext");
    sb3::write_sb3_file(&input, &p).unwrap();
    decompile_sb3(&input, Some(&output), false).unwrap();
    let rebuilt = sb3::read_sb3_bytes(&compile_entry_to_sb3_bytes(&output, true).unwrap()).unwrap();
    let blocks = rebuilt.project["targets"][1]["blocks"].as_object().unwrap();
    let proto = blocks
        .values()
        .find(|b| b["opcode"] == "procedures_prototype")
        .unwrap();
    let call = blocks
        .values()
        .find(|b| b["opcode"] == "procedures_call")
        .unwrap();
    assert_eq!(proto["mutation"]["proccode"], "move %b %s");
    assert_eq!(call["mutation"]["proccode"], proto["mutation"]["proccode"]);
    assert_eq!(proto["mutation"]["argumentdefaults"], "[false,\"\"]");
    assert_eq!(
        blocks
            .values()
            .filter(|b| b["opcode"] == "argument_reporter_boolean")
            .count(),
        2
    );
}
