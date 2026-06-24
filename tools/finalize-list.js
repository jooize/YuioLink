// Assemble the FINAL curated wordlist from the raw Union <=6 and every agreed
// exclusion across all rounds, then write tools/yuiolink-curated.txt (+ the
// excluded list for provenance) and report size/entropy.
const fs = require("fs");
const T = process.env.TMPDIR;
const data = JSON.parse(fs.readFileSync(T + "/curate_data.json", "utf8"));
const meta = {}; data.forEach(d => meta[d.w] = d);
const all = fs.readFileSync("tools/yuiolink-union-le6.txt", "utf8").trim().split("\n").map(s => s.trim()).filter(Boolean);
const readSet = f => new Set(fs.readFileSync(f, "utf8").trim().split(/\s+/).map(s => s.trim().toLowerCase()).filter(Boolean));

const round2 = readSet("tools/.excluded-paste-2.txt");           // card-reviewer pass (280 + agreed/aide/aids)
const finalExtra = readSet("tools/.excluded-final-extra.txt");    // brands/offensive/clinical/hard (52)
const weird = ["bulgur","gnat","nuclei","mauve","septum","folic","curtsy","recoup","bovine","sepia","obtuse","yodel"];
const tech  = ["cursor","debug","ebook","decal","spoof"];
const extra = ["ashy","hankie"];

const excl = new Set([...round2, ...finalExtra, ...weird, ...tech, ...extra]);
let keep = all.filter(w => excl.has(w) === false);

// drop redundant plurals (word + base both kept -> drop the +s)
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
plurals.forEach(w => excl.add(w));
keep = keep.filter(w => pset.has(w) === false);

keep.sort();
fs.writeFileSync("tools/yuiolink-curated.txt", keep.join("\n") + "\n");
fs.writeFileSync("tools/yuiolink-excluded.txt", [...excl].sort().join("\n") + "\n");

const n = keep.length, b = Math.log2(n);
const f = c => ((1 - Math.exp(-c * 604800 / Math.pow(n, 4))) * 100).toFixed(3) + "%";
console.log("FINAL curated list: " + n + " words  (excluded " + excl.size + " from " + all.length + ")");
console.log("  " + b.toFixed(2) + " bits/word");
console.log("  1 word = " + b.toFixed(1) + " bits (public)   4 words = " + (4 * b).toFixed(1) + " bits (single-use)");
console.log("  7-day single-use vs botnet 1e6/s: " + f(1e6) + "   vs fleet 1e4/s: " + f(1e4));
console.log("  written: tools/yuiolink-curated.txt, tools/yuiolink-excluded.txt");
