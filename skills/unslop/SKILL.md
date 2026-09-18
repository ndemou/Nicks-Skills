---
name: unslop
description: Remove common AI writing patterns from user-facing prose. Apply by default unless the user requests a conflicting style.
---

# Unslop

Edit user-facing prose to remove AI patterns. Preserve meaning, technical accuracy, quoted text, code, required terminology, required formats, and any style or voice the user explicitly requests. Treat the patterns below as strong defaults rather than absolute bans when following a rule would make the result less clear, natural, or precise.

## Process

1. Scan for the patterns below.
2. Rewrite. Preserve meaning, match intended tone.
3. Self-audit: "What makes this obviously AI generated?" Fix remaining tells.

## Patterns to detect and fix

### Content

1. **Superficial -ing phrases.** "highlighting...", "ensuring...", "reflecting...", "showcasing...", "fostering...". Delete or expand with real sources.

### Language

1. **AI vocabulary.** Crucial, delve, enduring, enhance, fostering, garner, interplay, intricate, landscape (abstract), pivotal, showcase, tapestry (abstract), testament, underscore, vibrant. Replace with plain words.
2. **Fancy ways to say "is".** "serves as", "stands as", "boasts", "features". Just say "is" or "has".
3. **"Not just X, but Y."** State the point directly instead.
4. **Rule of three.** Forcing ideas into groups of three. Use the natural number.
5. **Synonym cycling.** Protagonist, main character, central figure, hero all in one paragraph. Pick one, repeat it.
6. **False ranges.** "from X to Y" where X and Y aren't on a meaningful scale. List topics directly.

### Style

1. **Em dash overuse.** Avoid em dashes/en dashes where periods or commas work.
2. **Colon overuse.** Colons are fine before a list or example. Not as mid-sentence connectors. "If you're coming from traditional automation: instead of registering event handlers, you describe conditions" adds nothing with the colon. Rewrite to let the point stand on its own without comparison framing. "Describing when the scheduler should fire works best as plain English." Same meaning, no crutch punctuation.
3. **Boldface overuse.** Don't bold every proper noun or acronym.
4. **Inline-header lists.** Avoid a bold label and colon that restates the line: "**Performance:** Performance improved...". Convert those to prose. A bold lead-in that ends in a period, names the item, and is followed by genuinely new detail ("**Schema in TypeScript.** Tables live in one file.") is fine.
5. **Title case headings.** Use sentence case.
6. **Decorative emojis.** Remove from headings and bullets.

### Communication artifacts

1. **Chatbot phrases.** "I hope this helps!", "Let me know if...", "Of course!", "Certainly!", Remove them.
2. **Sycophantic tone.** "Great question! You're absolutely right!" Respond directly.

### Filler

1. **Filler phrases.** "In order to" becomes "To". "Due to the fact that" becomes "Because". 
2. **Excessive hedging.** "could potentially possibly be argued that it might" becomes "may".
3. **Generic conclusions.** "The future looks bright." State specific plans or facts.

### Jargon

1. **Abstract metaphor nouns.** Substrate, wedge, vector, locus, vantage, nexus, primitive (as noun), harness (as metaphor), surface (as in "API surface"), bedrock, scaffolding (as metaphor), modality, paradigm, gold-plating, ratchet (as metaphor), evacuate (for moving code), endgame, north star, flywheel. These read as technical but usually have a plainer concrete word. "Substrate" becomes "base". "Wedge in" becomes "add". "Vector" becomes "way" or "method". "Gold-plating" becomes "more than the job needs". "Ratchet" becomes the mechanism's real name or "a limit that only tightens". "Evacuate" becomes "move out". "Endgame" becomes "the last phase". Pick the concrete word.

### Plain speech

1. **Say what it does, not how it feels.** "the database stays close at hand", "SQL you can read", "types that follow your schema" name a feeling. The fix names the mechanism or a number: "`.toSQL()` returns the exact string sent to the database", "a column rename fails the build". Ask what the sentence tells the reader to do or know, then write that. If you can't restate it as a concrete instruction, fact, or number, cut it. One more check: if the sentence could appear unchanged in another project's docs, it says nothing about this one. Cut it.
2. **Shorten or split dense sentences.** If the reader has to backtrack to parse a sentence, break it in two or drop clauses. One idea per sentence.
3. **Active voice.** Prefer it. Catch "is/are/was/were + past participle" and name the actor: "queries are validated" becomes "the compiler validates queries", "the file is parsed by the loader" becomes "the loader parses the file". Passive is fine only when the actor is unknown or genuinely doesn't matter.
4. **Cut adverbs, or use a stronger verb.** "runs quickly" becomes "is fast" or the number. "significantly improves" becomes the measured delta. An adverb propping up a weak verb means the verb is wrong.
5. **Prefer the plain word.** "utilize" becomes "use", "leverage" becomes "use", "facilitate" becomes "help", "numerous" becomes "many", "in the event that" becomes "if". The fancier synonym is rarely clearer.
6. **Mannered prose.** Metaphor or flourish where a literal phrase exists: aphorisms ("wire it or delete it"), rhetorical fragments for effect, personified code ("the plan holds it"), figurative verbs ("rides along", "stands on"), stock framing phrases. "A dial worth turning" becomes "a parameter worth varying". Say what you mean. Rule 26 covers the metaphor nouns.
7. **Over-compression.** Dropped articles, verbless fragments, symbol-speak, and abbreviations that make the reader decode instead of read. "Parser rejects bad date → exit 2, no write" becomes "The parser rejects a bad date, exits with code 2, and writes nothing." Write whole sentences with their articles and verbs, and spell out arrows and abbreviations.
