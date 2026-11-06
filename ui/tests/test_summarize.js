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
assert.strictEqual(summarize.source([]), null)

// The open message is not read again, and long threads keep their end.
deepEqual(summarize.memberIds(["a", "b", "open", "b", "c"], "open", 20), ["a", "b", "c"])
deepEqual(summarize.memberIds(["a", "b", "c", "d"], "x", 2), ["c", "d"])
deepEqual(summarize.memberIds(null, "x", 2), [])
