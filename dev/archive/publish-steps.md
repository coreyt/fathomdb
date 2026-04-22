You need to configure (I can't do this for you):

1. CARGO_REGISTRY_TOKEN — crates.io API token
   - Generate at https://crates.io/settings/tokens (needs "publish new crates" scope)
   - Add at repo Settings > Secrets and variables > Actions > New repository secret
2. PyPI trusted publisher
   - Go to https://pypi.org/manage/account/publishing/ and add a new pending publisher:
     - PyPI project name: fathomdb
     - Owner: coreyt
     - Repository: fathomdb
     - Workflow name: release.yml
     - Environment name: pypi
   - Then in GitHub repo Settings > Environments, create an environment named pypi

Once those are configured, I'll tag and push v0.1.0 to trigger the release workflow.
