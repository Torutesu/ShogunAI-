/**
 * Deploy guard — the Worker has one writer, and it is GitHub Actions.
 *
 * Two pipelines writing to `shogunai-website` is last-writer-wins: a local
 * `wrangler deploy` silently replaces whatever Actions shipped, and there is no
 * preview environment to catch it. That already cost us a production rollback.
 *
 * This only guards the npm scripts. The real control is credentials: keep
 * CLOUDFLARE_API_TOKEN in GitHub Actions secrets and nowhere else, so a local
 * deploy has nothing to authenticate with.
 */
if (!process.env.GITHUB_ACTIONS) {
  console.error(`
  Refusing to deploy from outside GitHub Actions.

  Production deploys run from .github/workflows/deploy.yml on push to the
  release branch. To ship: open a PR, merge it, and let the workflow deploy.

  To preview the Workers build locally without deploying:
      pnpm --filter @shogun-ai/website cf:preview

  If you genuinely need a one-off manual deploy, run the workflow by hand
  (Actions -> Deploy website -> Run workflow) rather than deploying from here,
  so the deploy is still recorded against a commit.
`);
  process.exit(1);
}
