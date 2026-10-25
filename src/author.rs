//! Scaffolds ordinary widget packages. No renderer/interpreter is installed implicitly.
use crate::Result;
use std::{fs, path::Path};
const PYTHON: &str = r#"import os
import sys
sys.path.insert(0, os.environ['RS_WWIDGET_SDK'])
from rs_wwidget import Rect, Text, StateStore, emit_canvas, events

store = StateStore()
count = store.load({'count': 0})['count']

def draw():
    emit_canvas([
        Rect((24, 56, 272, 48), color='accent', radius=8, id='increment'),
        Text(f'Count: {count}', (24, 68, 272, 28), align='center', color='background'),
        Text('Click the button; state survives reloads', (24, 126, 272, 22), size=12, color='muted')
    ], title='My widget')

draw()
for event in events(timeout=5):
    changed = False
    if event and event['type'] == 'pointer' and event['kind'] == 'button':
        if event['pressed'] and event['button'] == 272 and event['target'] == 'increment':
            count += 1
            changed = True
    if event and event['type'] == 'custom' and event['name'] == 'reset':
        count = 0
        changed = True
    if changed:
        store.save({'count': count})
    draw()
"#;
const JAVASCRIPT: &str = r#"import readline from 'node:readline';
let count = 0;
function draw() {
  process.stdout.write(JSON.stringify({version:2,title:'JavaScript widget',elements:[
    {id:'increment',bounds:[24,56,272,48],content:{type:'rect',color:'accent',radius:8}},
    {bounds:[24,68,272,28],content:{type:'text',text:`Count: ${count}`,align:'center',color:'background'}}
  ]})+'\n');
}
draw();
const heartbeat=setInterval(draw,5000);
for await (const line of readline.createInterface({input:process.stdin})) {
  const event=JSON.parse(line);
  if(event.type==='pointer' && event.kind==='button' && event.pressed && event.button===272 && event.target==='increment') count++;
  if(event.type==='custom' && event.name==='reset') count=0;
  draw();
}
clearInterval(heartbeat);
"#;
pub fn create(args: &[String]) -> Result<()> {
    let (directory, backend) = match args {
        [directory] => (directory.as_str(), "python"),
        [directory, flag, backend] if flag == "--backend" => (directory.as_str(), backend.as_str()),
        _ => return Err("--init-widget DIRECTORY [--backend python|javascript|qml]".into()),
    };
    let directory = Path::new(directory);
    let id = directory
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| crate::registry::valid_id(s) && !crate::layout::NAMES.contains(s))
        .ok_or("directory must be a valid, non-built-in widget ID")?;
    let (filename, source, settings) = match backend {
        "python" => (
            "main.py",
            PYTHON.to_owned(),
            "command = [\"python3\", \"{widget_dir}/main.py\"]\ninput = \"interactive\"\nwatch_files = [\"main.py\"]\n",
        ),
        "javascript" => (
            "main.mjs",
            JAVASCRIPT.to_owned(),
            "command = [\"node\", \"{widget_dir}/main.mjs\"]\ninput = \"interactive\"\nwatch_files = [\"main.mjs\"]\n",
        ),
        "qml" => (
            "shell.qml",
            include_str!("../widgets/qml-example/shell.qml")
                .replace("rs-wwidget-qml-example", &format!("rs-wwidget-{id}")),
            "command = [\"quickshell\", \"-p\", \"{widget_dir}/shell.qml\"]\ninput = \"none\"\nrender_mode = \"external\"\n",
        ),
        _ => return Err("backend must be python, javascript or qml".into()),
    };
    let height = if backend == "qml" { 300 } else { 170 };
    let manifest = format!(
        "version = 2\nid = \"{id}\"\nprotocol = 2\nwidth = 320\nheight = {height}\n{settings}"
    );
    fs::create_dir(directory).map_err(|e| {
        format!(
            "{}: {e}; parent must exist and package must be new",
            directory.display()
        )
    })?;
    let result = (|| -> Result<()> {
        fs::write(directory.join(filename), source)?;
        fs::write(directory.join("widget.toml"), manifest)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(directory.join(filename));
        let _ = fs::remove_file(directory.join("widget.toml"));
        let _ = fs::remove_dir(directory);
    }
    result?;
    println!("Created {backend} widget '{id}': {}", directory.display());
    println!(
        "Add \"{id}\" to widgets in config.toml, then restart. Use widget {id} reload after edits, or enable watch_files."
    );
    Ok(())
}
