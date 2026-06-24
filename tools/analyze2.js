// Round-2 analysis:
//  1. diff new excluded list vs previous (what got removed this round)
//  2. plural pairs (word + word+s both present) -> how many we'd lose dropping the +s
//  3. apply ashy + hankie drops
//  4. surface the weirdest / hardest-to-pronounce kept words (rarest)
const fs = require("fs");
const T = process.env.TMPDIR;
const data = JSON.parse(fs.readFileSync(T + "/curate_data.json", "utf8"));
const meta = {}; data.forEach(d => meta[d.w] = d);
const allWords = data.map(d => d.w);

const readSet = f => new Set(fs.readFileSync(f, "utf8").trim().split(/\s+/).map(s => s.trim().toLowerCase()).filter(Boolean));
const oldX = readSet("tools/.excluded-paste.txt");
const newX = readSet("tools/.excluded-paste-2.txt");

const added   = [...newX].filter(w => oldX.has(w) === false).sort();
const rescued = [...oldX].filter(w => newX.has(w) === false).sort();

// final exclusion = new list + ashy + hankie
const finalX = new Set([...newX, "ashy", "hankie"]);
const keep = allWords.filter(w => finalX.has(w) === false);
const keepSet = new Set(keep);

// plural pairs: kept word ending -s/-es whose base is also kept
const plurals = [];
for (const w of keep) {
  if (w.length >= 4 && w.endsWith("s")) {
    const b1 = w.slice(0, -1);
    if (keepSet.has(b1)) plurals.push([w, b1]);
    else if (w.endsWith("es") && keepSet.has(w.slice(0, -2))) plurals.push([w, w.slice(0, -2)]);
  }
}
const keepNoPlural = keep.length - plurals.length;

function line(n){ const b=Math.log2(n); return n+" words  ->  "+b.toFixed(2)+" bits/word, 4-word "+(4*b).toFixed(1)+" bits, 7d-botnet "+((1-Math.exp(-1e6*604800/Math.pow(n,4)))*100).toFixed(3)+"%"; }

console.log("=== this round ===");
console.log("newly removed ("+added.length+"): "+added.join(", "));
console.log("un-excluded since last ("+rescued.length+"): "+(rescued.join(", ")||"none"));
console.log("\n=== plural pairs (drop the +s, keep base) ===");
console.log("count: "+plurals.length+"   examples: "+plurals.slice(0,40).map(p=>p[0]+"/"+p[1]).join(", "));
console.log("\n=== list size & entropy ===");
console.log("after new cuts + ashy + hankie:   "+line(keep.length));
console.log("also dropping the "+plurals.length+" plurals: "+line(keepNoPlural));
console.log("\n=== rarest kept words (weird-candidate pool, rank null or >25k) ===");
const weird = keep.map(w=>meta[w]).filter(d=>d && (d.rank==null || d.rank>25000))
  .sort((a,b)=>(b.rank==null?1e9:b.rank)-(a.rank==null?1e9:a.rank)).map(d=>d.w);
console.log("count: "+weird.length);
console.log(weird.slice(0,90).join(", "));

// save the full plural list for reference
fs.writeFileSync(T+"/plurals.txt", plurals.map(p=>p[0]).sort().join("\n"));
