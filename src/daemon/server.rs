use std::{collections::BTreeMap, process::Command};

use nparse::*;
use serde_json::Value;
use zbus::dbus_interface;

pub struct SystemResourcesDaemon;

#[dbus_interface(name = "me.nalux.Resources")]
impl SystemResourcesDaemon {
    pub fn ram_info(&self) -> Vec<BTreeMap<String, String>> {
        // Vec<(Locator, Bank Locator, Size, Form Factor, Type, Type Detail, Speed, Manufacturer)>
        let output = String::from_utf8(
            Command::new("dmidecode")
                .args(["--type", "17", "-q"])
                .output()
                .map(|x| x.stdout)
                .unwrap_or_default()
        )
        .unwrap_or_default()
        .split("\n\n")
        .map(|x| (*x).to_owned().indent_to_json().unwrap_or(Value::Null))
        .collect::<Vec<Value>>();
        let mut ret_vec: Vec<BTreeMap<String, String>> = Vec::new();
        for v in output {
            match v {
                Value::Null => (),
                Value::Bool(_) => unimplemented!(),
                Value::Number(_) => unimplemented!(),
                Value::String(_) => unimplemented!(),
                Value::Array(_) => (),
                Value::Object(_) => {
                    // don't add it to the ret_vec if it's empty
                    if v["Memory Device"]["Size"]
                        .as_str()
                        .unwrap_or("No Module Installed")
                        != "No Module Installed"
                    {
                        let mut hash_map: BTreeMap<String, String> = BTreeMap::new();
                        hash_map.insert(
                            "Locator".into(),
                            v["Memory Device"]["Locator"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                        );
                        hash_map.insert(
                            "Bank Locator".into(),
                            v["Memory Device"]["Bank Locator"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                        );
                        hash_map.insert(
                            "Size".into(),
                            v["Memory Device"]["Size"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                        );
                        hash_map.insert(
                            "Form Factor".into(),
                            v["Memory Device"]["Form Factor"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                        );
                        hash_map.insert(
                            "Type".into(),
                            v["Memory Device"]["Type"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                        );
                        hash_map.insert(
                            "Type Detail".into(),
                            v["Memory Device"]["Type Detail"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                        );
                        hash_map.insert(
                            "Speed".into(),
                            v["Memory Device"]["Speed"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                        );
                        hash_map.insert(
                            "Manufacturer".into(),
                            v["Memory Device"]["Manufacturer"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                        );
                        ret_vec.push(hash_map);
                    }
                }
            }
        }
        ret_vec
    }
}
