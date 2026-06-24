// Diff the user's final excluded list against my original 291 proposals,
// to report which proposed words they RESCUED (kept).
const fs = require("fs");
const T = process.env.TMPDIR;
const data = JSON.parse(fs.readFileSync(T + "/curate_data.json", "utf8"));
const proposed = data.filter(d => d.proposed).map(d => d.w);
const excluded = new Set(
  fs.readFileSync("tools/.excluded-paste.txt", "utf8").trim().split(/\s+/).map(s => s.trim().toLowerCase()).filter(Boolean)
);
const kept = proposed.filter(w => excluded.has(w) === false);
const notProposed = [...excluded].filter(w => proposed.includes(w) === false);

console.log("proposed: " + proposed.length + "  |  you excluded: " + excluded.size + "  |  you rescued (kept): " + kept.length);
console.log("\n=== KEPT (rescued from my 291) ===");
console.log(kept.sort().join(", "));
if (notProposed.length) {
  console.log("\n=== in your excluded list but NOT one of my 291 (you added these) ===");
  console.log(notProposed.sort().join(", "));
}
