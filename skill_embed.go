// Package devrig embeds repo-level assets (the Claude Code skill) so the
// internal/commands package can serve them. go:embed paths cannot traverse
// outside a package directory, so this lives at the module root next to skill/.
package devrig

import _ "embed"

//go:embed skill/claude-code/SKILL.md
var SkillMD string

//go:embed skill/claude-code/reference/configuration.md
var SkillReferenceConfigurationMD string
