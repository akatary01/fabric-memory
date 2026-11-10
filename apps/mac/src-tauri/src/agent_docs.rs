use serde_json::{Value,json};
const FILES:&[(&str,&str)]=&[
("AGENTS.md",include_str!("../../../../AGENTS.md")),
("README.md",include_str!("../../../../README.md")),
("docs/AGENT.md",include_str!("../../../../docs/AGENT.md")),
("docs/ARCHITECTURE.md",include_str!("../../../../docs/ARCHITECTURE.md")),
("docs/BEACON.md",include_str!("../../../../docs/BEACON.md")),
("docs/CODING-AGENT.md",include_str!("../../../../docs/CODING-AGENT.md")),
("docs/DISTRIBUTION.md",include_str!("../../../../docs/DISTRIBUTION.md")),
("docs/LICENSES.md",include_str!("../../../../docs/LICENSES.md")),
("docs/LOCAL-LLM.md",include_str!("../../../../docs/LOCAL-LLM.md")),
("docs/MAC-ROUTING-WEB.md",include_str!("../../../../docs/MAC-ROUTING-WEB.md")),
("docs/PHONE-LOCAL-LLM.md",include_str!("../../../../docs/PHONE-LOCAL-LLM.md")),
("docs/SYNC.md",include_str!("../../../../docs/SYNC.md")),
("docs/TESTFLIGHT.md",include_str!("../../../../docs/TESTFLIGHT.md"))];
pub fn context()->String{format!("Fabric coding tools operate in the selected workspace. Inspect relevant files and scoped project guidance, make the requested changes, run meaningful checks, and report actual outcomes. Tool paths are relative; use '.' to list the folder. Do not repeat unchanged failed actions. Read only relevant bundled guides with fabric_docs when needed: {}",FILES.iter().map(|(name,_)|*name).collect::<Vec<_>>().join(", "))}
pub fn read(name:&str)->Result<Value,String>{let (_,text)=FILES.iter().find(|(path,_)|*path==name).ok_or("Unknown bundled guide. Use the exact name from the guide index.")?;Ok(json!({"path":name,"text":text.chars().take(18000).collect::<String>(),"truncated":text.chars().count()>18000}))}
pub fn tool()->Value{json!({"name":"fabric_docs","description":"Read relevant bundled Fabric agent/developer Markdown guides. These guides are public application documentation.","inputSchema":{"type":"object","properties":{"path":{"type":"string","enum":FILES.iter().map(|(name,_)|*name).collect::<Vec<_>>()}},"required":["path"]}})}
#[cfg(test)]mod tests{use super::*;#[test]fn bundled_guides_are_available_without_a_source_checkout(){for name in ["AGENTS.md","README.md","docs/CODING-AGENT.md","docs/AGENT.md","docs/ARCHITECTURE.md"]{assert!(!read(name).unwrap()["text"].as_str().unwrap().is_empty());}assert!(read("../../private.md").is_err());assert!(context().contains("fabric_docs"));}}
