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
("docs/SYNC.md",include_str!("../../../../docs/SYNC.md"))];
pub fn setup() -> &'static str {
    include_str!("../../../../docs/AGENT-SETUP.md")
}
pub fn for_task(coding: bool, visual: bool, computer: bool, web: bool) -> String {
    let mut guides = vec![setup()];
    if coding {
        guides.extend([
            include_str!("../../../../docs/AGENT-CODING-WORKFLOW.md"),
            include_str!("../../../../docs/AGENT-DEBUGGING.md"),
            include_str!("../../../../docs/AGENT-WORKSPACE.md"),
        ]);
    }
    if visual { guides.push(include_str!("../../../../docs/AGENT-VISUALIZATION.md")); }
    if computer {
        guides.extend([
            include_str!("../../../../docs/AGENT-COMPUTER.md"),
            include_str!("../../../../docs/AGENT-RECORDING.md"),
        ]);
    }
    if web { guides.push(include_str!("../../../../docs/AGENT-WEB.md")); }
    guides.join("\n")
}
pub fn read(name:&str)->Result<Value,String>{let (_,text)=FILES.iter().find(|(path,_)|*path==name).ok_or("Unknown bundled guide. Use the exact name from the guide index.")?;Ok(json!({"path":name,"text":text.chars().take(18000).collect::<String>(),"truncated":text.chars().count()>18000}))}
#[cfg(test)]mod tests{use super::*;#[test]fn bundled_guides_are_available_without_a_source_checkout(){for name in ["AGENTS.md","README.md","docs/CODING-AGENT.md","docs/AGENT.md","docs/ARCHITECTURE.md"]{assert!(!read(name).unwrap()["text"].as_str().unwrap().is_empty());}assert!(read("../../private.md").is_err());assert!(for_task(true,false,false,false).contains("Coding workflow"));}}
