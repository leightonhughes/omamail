const assert = require("assert")
const { load, deepEqual } = require("./load.js")

const summarize = load("compose/Summarize.js")

const kate = { from: { name: "Kate Smith", email: "kate@x.org", display: "Kate Smith" },
  date: "2010-01-14T22:20:00.000Z", fullTime: "Jan 15, 2010 09:20" }
deepEqual(summarize.entry(kate, "Maple?"),
  { name: "Kate Smith", email: "kate@x.org", date: "2010-01-14T22:20:00.000Z", when: "Jan 15, 2010 09:20", body: "Maple?" })
// The reader's summaries carry a Date (and dateMs), not the wire string.
assert.strictEqual(summarize.entry({ date: new Date("2026-11-06T04:47:51Z") }, "").date, "2026-11-06T04:47:51.000Z")
assert.strictEqual(summarize.entry({ dateMs: 1263507600000, date: new Date(0) }, "").date, "2010-01-14T22:20:00.000Z")
assert.strictEqual(summarize.entry({}, "").date, "")
// A sender with no name falls back to what the list shows.
assert.strictEqual(summarize.entry({ from: { email: "a@x.org", display: "a" } }, "").name, "a")

// The open message arrives twice: once from the reader, once from the thread.
const once = summarize.entry(kate, "Maple?")
const ben = summarize.entry({ from: { name: "Ben", email: "b@x.org" }, date: "2010-01-14T22:18:00.000Z" }, "Birch?")
const out = summarize.source([once, ben, summarize.entry(kate, "Maple?")])
assert.strictEqual(out.messages.length, 2)
assert.strictEqual(out.newestFirst, true)
assert.strictEqual(summarize.source([ben], false).newestFirst, false)
// Only a forward's source is marked as one.
assert.strictEqual(summarize.source([ben], true, true).forward, true)
assert.strictEqual("forward" in summarize.source([ben]), false)
assert.strictEqual(summarize.source([]), null)

// The open message is not read again, and long threads keep their end.
deepEqual(summarize.memberIds(["a", "b", "open", "b", "c"], "open", 20), ["a", "b", "c"])
deepEqual(summarize.memberIds(["a", "b", "c", "d"], "x", 2), ["c", "d"])
deepEqual(summarize.memberIds(null, "x", 2), [])

// A late thread swaps the placed quote in place, around what was typed.
const oldQ = "> Kate\n> Maple?"
const newQ = "> Kate\n> Maple?\n>\n> Ben\n> Birch?"
const typed = "Oak, I think.\n\nCheers\n\n" + oldQ
deepEqual(summarize.requote(typed, oldQ, newQ, 13), { at: 23, end: 23 + oldQ.length, cursor: 13 })
// A cursor after the quote moves with it; one inside it stays at its start.
assert.strictEqual(summarize.requote(typed + "\nPS", oldQ, newQ, typed.length + 3).cursor, typed.length + 3 + newQ.length - oldQ.length)
assert.strictEqual(summarize.requote(typed, oldQ, newQ, 26).cursor, 23)
// Edited inside the quote, or quoted twice: leave the body alone.
assert.strictEqual(summarize.requote(typed.replace("Maple", "Elm"), oldQ, newQ, 0), null)
assert.strictEqual(summarize.requote(typed + oldQ, oldQ, newQ, 0), null)
assert.strictEqual(summarize.requote(typed, "", newQ, 0), null)
