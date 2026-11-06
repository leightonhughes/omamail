.pragma library

// Presentation rules for Summarize replies. The format itself, and what is
// stripped from each message, belong to src/message/summarize.rs; this only
// gathers what the backend is given.

// One message of a Summarize reply, from its summary and its readable text.
function entry(summary, text) {
  var message = summary || ({})
  var from = message.from || ({})
  return {
    name: String(from.name || from.display || ""),
    email: String(from.email || ""),
    date: String(message.date || ""),
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
