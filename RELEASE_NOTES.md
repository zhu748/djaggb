# Release Notes

## Fix bug
- Fix Claude Code usage counters not resetting to zero when session/7-day cooldown windows expire without a new request (session, weekly, Opus, Sonnet buckets now clear on periodic checks).
