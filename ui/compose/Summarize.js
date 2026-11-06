.pragma library

// Presentation rules for Summarize replies. The format itself, and what is
// stripped from each message, belong to src/message/summarize.rs; this only
// gathers what the backend is given.

// One message of a Summarize reply, from its summary and its readable text.
function entry(summary, text) {
  var message = summary || ({})
  var from = message.from || ({})
  // The reader hydrates `date` into a Date (MailAccount.hydrateSummary), whose
  // string form is not RFC 3339; summarize.rs dates and orders by RFC 3339.
  var at = Number(message.dateMs)
  if (!(at > 0) && message.date && typeof message.date.getTime === "function") at = message.date.getTime()
  if (!(at > 0) && message.date) at = Date.parse(String(message.date))
  return {
    name: String(from.name || from.display || ""),
    email: String(from.email || ""),
    date: at > 0 ? new Date(at).toISOString() : "",
    when: String(message.fullTime || ""),
    body: String(text || "")
  }
}

// What `message.composeText` and `message.compose` take as `summarize` /
// `summary`, or null when there is nothing to summarize. A message read twice
// counts once.
function source(entries, newestFirst) {
  var list = []
  var seen = ({})
  var all = entries || []
  for (var i = 0; i < all.length; i++) {
    var item = all[i]
    if (!item) continue
    var key = item.email + "\n" + item.date + "\n" + item.body
    if (seen[key]) continue
    seen[key] = true
    list.push(item)
  }
  return list.length > 0 ? ({ messages: list, newestFirst: newestFirst !== false }) : null
}

// The other members of the conversation to read: the last `limit`, without
// the message already open. The backend keeps the newest five by date.
function memberIds(ids, openId, limit) {
  var out = []
  var list = ids || []
  for (var i = 0; i < list.length; i++) {
    var id = String(list[i] || "")
    if (id !== "" && id !== String(openId || "") && out.indexOf(id) === -1) out.push(id)
  }
  var keep = Number(limit) > 0 ? Number(limit) : 20
  return out.length > keep ? out.slice(out.length - keep) : out
}

// Where to swap a placed quote for a longer one: the old quote's span, when it
// is in the body exactly once and untouched, and where the cursor goes so it
// stays beside what was typed. Null leaves the body as it is.
function requote(body, oldQuote, newQuote, cursor) {
  var text = String(body || "")
  var old = String(oldQuote || "")
  var next = String(newQuote || "")
  if (old === "" || next === "") return null
  var at = text.indexOf(old)
  if (at < 0 || text.indexOf(old, at + 1) >= 0) return null
  var end = at + old.length
  var place = Number(cursor) || 0
  return { at: at, end: end, cursor: place >= end ? place + next.length - old.length : Math.min(place, at) }
}
