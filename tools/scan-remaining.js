// Apply the agreed drops, then dump the rarest survivors so we can hunt more
// jargon / obscure / hard words. "Anyone can use this" = prefer universally
// recognizable words.
const fs = require("fs");
const T = process.env.TMPDIR;
const data = JSON.parse(fs.readFileSync(T + "/curate_data.json", "utf8"));
const meta = {}; data.forEach(d => meta[d.w] = d);
const allWords = data.map(d => d.w);
const readSet = f => new Set(fs.readFileSync(f, "utf8").trim().split(/\s+/).map(s => s.trim().toLowerCase()).filter(Boolean));

const newX = readSet("tools/.excluded-paste-2.txt");
const weird = ["bulgur","gnat","nuclei","mauve","septum","folic","curtsy","recoup","bovine","sepia","obtuse","yodel"];
const tech  = ["cursor","debug","ebook","decal","spoof"];
const extra = ["ashy","hankie"];
const finalX = new Set([...newX, ...weird, ...tech, ...extra]);

let keep = allWords.filter(w => finalX.has(w) === false);
const keepSet = new Set(keep);
const plurals = [];
for (const w of keep) {
  if (w.length >= 4 && w.endsWith("s")) {
    const b1 = w.slice(0, -1);
    if (keepSet.has(b1)) plurals.push(w);
    else if (w.endsWith("es") && keepSet.has(w.slice(0, -2))) plurals.push(w);
  }
}
const pset = new Set(plurals);
keep = keep.filter(w => pset.has(w) === false);

function line(n){ const b=Math.log2(n); return n+" words -> "+b.toFixed(2)+" bits/word, 4-word "+(4*b).toFixed(1)+" bits, 7d-botnet "+((1-Math.exp(-1e6*604800/Math.pow(n,4)))*100).toFixed(3)+"%"; }
console.log("after all agreed drops (incl "+plurals.length+" plurals): " + line(keep.length));

// rarest survivors, with rank, in rarity order
const rare = keep.map(w => meta[w]).filter(d => d && (d.rank == null || d.rank > 14000))
  .sort((a,b) => (b.rank==null?1e9:b.rank) - (a.rank==null?1e9:a.rank));
console.log("\nrare survivors (rank null or >14k): " + rare.length + "\n");
// print in lines of ~14 for readability
let buf = [];
rare.forEach(d => { buf.push(d.w); });
for (let i=0;i<buf.length;i+=14) console.log(buf.slice(i,i+14).join(", "));

fs.writeFileSync(T + "/keep_now.txt", keep.sort().join("\n"));
