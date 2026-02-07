### GitHub CLI workflow (milestones, tickets, PRs)

This repo is set up for **GitHub CLI-first** workflow using `gh`.

Pre-reqs:
- `gh auth status` shows you’re logged in
- repo remote is set (`git remote -v`)

## 1) Templates: how they work

The repo contains:
- Issue templates: `.github/ISSUE_TEMPLATE/*.yml`
- PR template: `.github/PULL_REQUEST_TEMPLATE.md`

Once these files are **committed and pushed**, GitHub uses them automatically:
- UI: Issues → “New issue” shows template choices
- CLI: `gh issue create --template <filename>.yml`
- PRs: opening a PR will prefill from the PR template

## 2) Milestones (CLI)

GitHub doesn’t have a built-in `gh milestone create` command, so use the API.

Create milestones (example: A–G from `docs/public_testnet_plan.md`):

```bash
REPO="catalyst-network/catalyst-node-rust"

gh api -X POST "repos/$REPO/milestones" -f title="Milestone A — Spec pinning + canonical boundaries"
gh api -X POST "repos/$REPO/milestones" -f title="Milestone B — Transaction pipeline"
gh api -X POST "repos/$REPO/milestones" -f title="Milestone C — Membership / worker pool"
gh api -X POST "repos/$REPO/milestones" -f title="Milestone D — 4-phase consensus (protocol-faithful)"
gh api -X POST "repos/$REPO/milestones" -f title="Milestone E — State application + authenticated queries"
gh api -X POST "repos/$REPO/milestones" -f title="Milestone F — Public testnet ops + safety defaults"
gh api -X POST "repos/$REPO/milestones" -f title="Milestone G — Testing + release gate"
```

List milestones:

```bash
gh api "repos/$REPO/milestones" --paginate -q '.[] | "\(.number)\t\(.title)\t\(.state)"'
```

## 3) Creating issues (CLI, non-interactive)

The `--template` flow is interactive. For repeatable automation (what we’ll use together),
create issues with a prebuilt body.

Example (spec ticket):

```bash
REPO="catalyst-network/catalyst-node-rust"
MILESTONE_TITLE="Milestone A — Spec pinning + canonical boundaries"

cat > /tmp/issue.md <<'EOF'
Spec reference (section/page):
- Consensus v1.2 §4.5 (Transaction validity), p.__–__

Problem statement:
- <what’s missing/incorrect today>

Scope (in/out):
In:
- ...
Out:
- ...

Acceptance criteria:
- ...

Test plan:
- Unit:
- Local testnet:
  - make testnet-down
  - make testnet-up
  - make testnet-status
  - make testnet-contract-test
  - make testnet-down

Code areas (expected):
- crates/...
EOF

gh issue create \
  --repo "$REPO" \
  --title "[Spec] <short description>" \
  --body-file /tmp/issue.md \
  --label spec \
  --milestone "$MILESTONE_TITLE"
```

## 4) Commit/push cadence (recommended)

- One PR per coherent change (small, reviewable).
- For “testnet-impacting” changes, use as a gate:
  - `make testnet-down && make testnet-up && make testnet-status && make testnet-contract-test && make testnet-down`
- Push early; open a draft PR if needed; keep commits frequent.

