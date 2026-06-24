// Diagnostic: show Large-only words by frequency band so we can see which bands
// are genuinely droppable vs full of good words.
const fs = require("fs");
const T = process.env.TMPDIR;
function rd(f, col){ return fs.readFileSync(f,"utf8").trim().split("\n").map(l=>{const p=l.split("\t");return (col?p[1]:p[0]).trim().toLowerCase();}).filter(Boolean); }
const short1=new Set(rd("core/src/eff_short.txt",0)), bip39=new Set(rd(T+"/bip39.txt",0));
const words=rd("tools/yuiolink-union-le6.txt",0);
const freq=new Map();
fs.readFileSync(T+"/en_50k.txt","utf8").trim().split("\n").forEach((l,i)=>{const w=l.split(" ")[0].toLowerCase(); if(w&&!freq.has(w))freq.set(w,i+1);});
const largeOnly=words.filter(w=>!short1.has(w)&&!bip39.has(w));
function band(w){ const r=freq.get(w); if(r==null)return "absent"; if(r>40000)return "40-50k"; if(r>30000)return "30-40k"; if(r>20000)return "20-30k"; if(r>13000)return "13-20k"; return "<=13k"; }
const groups={};
for(const w of largeOnly){ (groups[band(w)] ||= []).push(w); }
for(const b of ["absent","40-50k","30-40k","20-30k","13-20k","<=13k"]){
  const g=groups[b]||[];
  console.log(`\n=== ${b}  (${g.length} words) ===`);
  console.log(g.slice(0,60).join(", "));
}
