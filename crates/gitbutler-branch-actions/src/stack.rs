use std::collections::HashMap;

use anyhow::{Context, Result};
use gitbutler_command_context::CommandContext;
use gitbutler_commit::commit_ext::CommitExt;
use gitbutler_oplog::entry::{OperationKind, SnapshotDetails};
use gitbutler_oplog::{OplogExt, SnapshotExt};
use gitbutler_oxidize::{ObjectIdExt, OidExt, RepoExt};
use gitbutler_reference::normalize_branch_name;
use gitbutler_repo_actions::RepoActionsExt;
use gitbutler_stack::{PatchReferenceUpdate, StackBranch};
use gitbutler_stack::{Stack, StackId, Target};
use serde::{Deserialize, Serialize};

use crate::actions::Verify;
use crate::dependencies::{commit_dependencies_from_stack, StackDependencies};
use crate::{
    commit::{commit_to_vbranch_commit, VirtualBranchCommit},
    r#virtual::{CommitData, IsCommitIntegrated, PatchSeries},
    VirtualBranchesExt,
};
use gitbutler_operating_modes::assure_open_workspace_mode;

/// Adds a new "series/branch" to the Stack.
/// This is in fact just creating a new  GitButler patch reference (head) and associates it with the stack.
/// The name cannot be the same as existing git references or existing patch references.
/// The target must reference a commit (or change) that is part of the stack.
/// The branch name must be a valid reference name (i.e. can not contain spaces, special characters etc.)
///
/// When creating heads, it is possible to have multiple heads that point to the same patch/commit.
/// If this is the case, the order can be disambiguated by specifying the `preceding_head`.
/// If there are multiple heads pointing to the same patch and `preceding_head` is not specified,
/// that means the new head will be first in order for that patch.
/// The argument `preceding_head` is only used if there are multiple heads that point to the same patch, otherwise it is ignored.
pub fn create_branch(
    ctx: &CommandContext,
    stack_id: StackId,
    req: CreateSeriesRequest,
) -> Result<()> {
    let mut guard = ctx.project().exclusive_worktree_access();
    ctx.verify(guard.write_permission())?;
    let _ = ctx.snapshot_create_dependent_branch(&req.name, guard.write_permission());
    assure_open_workspace_mode(ctx).context("Requires an open workspace mode")?;
    let mut stack = ctx.project().virtual_branches().get_stack(stack_id)?;
    let normalized_head_name = normalize_branch_name(&req.name)?;
    let repo = ctx.gix_repo()?;
    // If target_patch is None, create a new head that points to the top of the stack (most recent patch)
    if let Some(target_patch) = req.target_patch {
        stack.add_series(
            ctx,
            StackBranch::new(target_patch, normalized_head_name, req.description, &repo)?,
            req.preceding_head,
        )
    } else {
        stack.add_series_top_of_stack(ctx, normalized_head_name, req.description)
    }
}

/// Request to create a new series in a stack
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreateSeriesRequest {
    /// Name of the new series
    pub name: String,
    /// Description of the new series - can be markdown or anything really
    pub description: Option<String>,
    /// The target patch (head) to create these series for. If let None, the new series will be at the top of the stack
    pub target_patch: Option<gitbutler_stack::CommitOrChangeId>,
    /// The name of the series that preceded the newly created series.
    /// This is used to disambiguate the order when they point to the same patch
    pub preceding_head: Option<String>,
}

/// Removes series grouping from the Stack. This will not touch the patches / commits contained in the series.
/// The very last branch (reference) cannot be removed (A Stack must always contain at least one reference)
/// If there were commits/changes that were *only* referenced by the removed branch,
/// those commits are moved to the branch underneath it (or more accurately, the preceding it)
pub fn remove_branch(ctx: &CommandContext, stack_id: StackId, branch_name: String) -> Result<()> {
    let mut guard = ctx.project().exclusive_worktree_access();
    ctx.verify(guard.write_permission())?;
    let _ = ctx.snapshot_remove_dependent_branch(&branch_name, guard.write_permission());
    assure_open_workspace_mode(ctx).context("Requires an open workspace mode")?;
    let mut stack = ctx.project().virtual_branches().get_stack(stack_id)?;
    stack.remove_branch(ctx, branch_name)
}

/// Updates the name an existing branch and resets the pr_number to None.
/// Same invariants as `create_branch` apply.
pub fn update_branch_name(
    ctx: &CommandContext,
    stack_id: StackId,
    branch_name: String,
    new_name: String,
) -> Result<()> {
    let mut guard = ctx.project().exclusive_worktree_access();
    ctx.verify(guard.write_permission())?;
    let _ = ctx.snapshot_update_dependent_branch_name(&branch_name, guard.write_permission());
    assure_open_workspace_mode(ctx).context("Requires an open workspace mode")?;
    let mut stack = ctx.project().virtual_branches().get_stack(stack_id)?;
    let normalized_head_name = normalize_branch_name(&new_name)?;
    stack.update_branch(
        ctx,
        branch_name,
        &PatchReferenceUpdate {
            name: Some(normalized_head_name),
            ..Default::default()
        },
    )
}

/// Updates the description of an existing series in the stack.
/// The description can be set to `None` to remove it.
pub fn update_branch_description(
    ctx: &CommandContext,
    stack_id: StackId,
    branch_name: String,
    description: Option<String>,
) -> Result<()> {
    let mut guard = ctx.project().exclusive_worktree_access();
    ctx.verify(guard.write_permission())?;
    let _ = ctx.create_snapshot(
        SnapshotDetails::new(OperationKind::UpdateDependentBranchDescription),
        guard.write_permission(),
    );
    assure_open_workspace_mode(ctx).context("Requires an open workspace mode")?;
    let mut stack = ctx.project().virtual_branches().get_stack(stack_id)?;
    stack.update_branch(
        ctx,
        branch_name,
        &PatchReferenceUpdate {
            description: Some(description),
            ..Default::default()
        },
    )
}

/// Sets the forge identifier for a given series/branch. Existing value is overwritten.
///
/// # Errors
/// This method will return an error if:
///  - The series does not exist
///  - The stack cant be found
///  - The stack has not been initialized
///  - The project is not in workspace mode
///  - Persisting the changes failed
pub fn update_branch_pr_number(
    ctx: &CommandContext,
    stack_id: StackId,
    branch_name: String,
    pr_number: Option<usize>,
) -> Result<()> {
    let mut guard = ctx.project().exclusive_worktree_access();
    ctx.verify(guard.write_permission())?;
    let _ = ctx.create_snapshot(
        SnapshotDetails::new(OperationKind::UpdateDependentBranchPrNumber),
        guard.write_permission(),
    );
    assure_open_workspace_mode(ctx).context("Requires an open workspace mode")?;
    let mut stack = ctx.project().virtual_branches().get_stack(stack_id)?;
    stack.set_pr_number(ctx, &branch_name, pr_number)
}

/// Pushes all series in the stack to the remote.
/// This operation will error out if the target has no push remote configured.
pub fn push_stack(
    ctx: &CommandContext,
    stack_id: StackId,
    with_force: bool,
    branch_limit: Option<String>,
) -> Result<()> {
    ctx.verify(ctx.project().exclusive_worktree_access().write_permission())?;
    assure_open_workspace_mode(ctx).context("Requires an open workspace mode")?;
    let state = ctx.project().virtual_branches();
    let stack = state.get_stack(stack_id)?;

    let repo = ctx.repo();
    let default_target = state.get_default_target()?;
    let merge_base = repo.find_commit(repo.merge_base(
        stack.head_oid(&repo.to_gix()?)?.to_git2(),
        default_target.sha,
    )?)?;
    // let merge_base: CommitOrChangeId = merge_base.into();

    // First fetch, because we dont want to push integrated series
    ctx.fetch(
        &default_target.push_remote_name(),
        Some("push_stack".into()),
    )?;
    let gix_repo = ctx.gix_repo_for_merging_non_persisting()?;
    let cache = gix_repo.commit_graph_if_enabled()?;
    let mut graph = gix_repo.revision_graph(cache.as_ref());
    let mut check_commit = IsCommitIntegrated::new(ctx, &default_target, &gix_repo, &mut graph)?;
    let stack_branches = stack.branches();
    for branch in stack_branches {
        if branch.archived {
            // Nothing to push for this one
            continue;
        }
        if branch.head_oid(&gix_repo)? == merge_base.id().to_gix() {
            // Nothing to push for this one
            continue;
        }
        if branch_integrated(&mut check_commit, &branch, repo, &gix_repo)? {
            // Already integrated, nothing to push
            continue;
        }
        let push_details = stack.push_details(ctx, branch.name().to_owned())?;
        ctx.push(
            push_details.head,
            &push_details.remote_refname,
            with_force,
            None,
            Some(Some(stack.id)),
        )?;
        if let Some(limit) = &branch_limit {
            // Push only up to the specified branch limit (inclusive)
            if branch.name().eq(limit) {
                break;
            }
        }
    }
    Ok(())
}

pub(crate) fn branch_integrated(
    check_commit: &mut IsCommitIntegrated,
    branch: &StackBranch,
    repo: &git2::Repository,
    gix_repo: &gix::Repository,
) -> Result<bool> {
    if branch.archived {
        return Ok(true);
    }
    let oid = branch.head_oid(gix_repo)?;
    let branch_head = repo.find_commit(oid.to_git2())?;
    check_commit.is_integrated(&branch_head)
}

/// Returns the stack series for the API.
/// Newest first, oldest last in the list
/// `commits` is used to accelerate the is-integrated check.
pub(crate) fn stack_series(
    ctx: &CommandContext,
    stack: &mut Stack,
    default_target: &Target,
    check_commit: &mut IsCommitIntegrated,
    stack_dependencies: StackDependencies,
) -> (Vec<Result<PatchSeries, serde_error::Error>>, bool) {
    let mut requires_force = false;
    let mut api_series: Vec<Result<PatchSeries, serde_error::Error>> = vec![];
    for stack_branch in stack.branches() {
        let (api_branch_result, force) = stack_branch_to_api_branch(
            ctx,
            stack_branch,
            stack,
            default_target,
            check_commit,
            &stack_dependencies,
            &api_series
                .iter()
                .filter_map(|series| series.as_ref().ok())
                .collect::<Vec<_>>(),
        )
        .map_or_else(
            |err| {
                tracing::error!("Series Error: {}", err);
                (Err(err), false)
            },
            |(patch_series, force)| (Ok(patch_series), force),
        );
        if force {
            requires_force = true;
        }
        api_series.push(api_branch_result.map_err(|err| serde_error::Error::new(&*err)));
    }
    api_series.reverse();

    (api_series, requires_force)
}

#[allow(clippy::too_many_arguments)]
fn stack_branch_to_api_branch(
    ctx: &CommandContext,
    stack_branch: StackBranch,
    stack: &Stack,
    default_target: &Target,
    check_commit: &mut IsCommitIntegrated,
    stack_dependencies: &StackDependencies,
    parent_series: &[&PatchSeries],
) -> Result<(PatchSeries, bool)> {
    let mut requires_force = false;
    let repo = ctx.repo();
    let branch_commits = stack_branch.commits(ctx, stack)?;
    let remote = default_target.push_remote_name();
    let upstream_reference = if stack_branch.pushed(remote.as_str(), repo) {
        Some(stack_branch.remote_reference(remote.as_str()))
    } else {
        None
    };
    let mut patches: Vec<VirtualBranchCommit> = vec![];
    let mut is_integrated = false;

    let remote_commit_data = branch_commits
        .remote_commits
        .iter()
        .filter_map(|commit| {
            let data = CommitData::try_from(commit).ok()?;
            Some((data, commit.id()))
        })
        .collect::<HashMap<_, _>>();

    // Reverse first instead of later, so that we catch the first integrated commit
    for commit in branch_commits.clone().local_commits.iter().rev() {
        if !is_integrated {
            is_integrated = check_commit.is_integrated(commit)?;
        }
        let copied_from_remote_id = CommitData::try_from(commit)
            .ok()
            .and_then(|data| remote_commit_data.get(&data).copied());

        // A commit is local and remote only if it is's ID is in the list of remote
        // commits.
        let is_local_and_remote = branch_commits
            .remote_commits
            .iter()
            .any(|remote_commit| remote_commit.id() == commit.id());

        let remote_commit_id = if is_local_and_remote {
            None
        } else {
            commit
                .change_id()
                .and_then(|change_id| {
                    let matching_remote_commit = branch_commits
                        .remote_commits
                        .iter()
                        .find(|c| (c.change_id().as_deref() == Some(&change_id)))?;

                    Some(matching_remote_commit.id())
                })
                .or(copied_from_remote_id)
        };

        if remote_commit_id.is_some_and(|id| commit.id() != id) {
            requires_force = true;
        }

        let commit_dependencies = commit_dependencies_from_stack(stack_dependencies, commit.id());

        let vcommit = commit_to_vbranch_commit(
            repo,
            stack,
            commit,
            is_integrated,
            false,
            is_local_and_remote,
            copied_from_remote_id,
            remote_commit_id,
            commit_dependencies,
        )?;
        patches.push(vcommit);
    }
    // There should be no duplicates, but dedup because the UI cant handle duplicates
    patches.dedup_by(|a, b| a.id == b.id);

    let mut upstream_patches = vec![];
    for commit in branch_commits.remote_commits.iter().rev() {
        if patches
            .iter()
            .any(|p| p.id == commit.id() || p.remote_commit_id == Some(commit.id()))
        {
            // Skip if we already have this commit in the list
            continue;
        }

        if parent_series.iter().any(|series| {
            if series.archived {
                return false;
            };

            series
                .patches
                .iter()
                .any(|p| p.id == commit.id() || p.remote_commit_id == Some(commit.id()))
        }) {
            // Skip if we already have this commit in the list
            continue;
        }

        let is_integrated = {
            if parent_series.iter().any(|series| {
                if !series.archived {
                    return false;
                };

                series.upstream_patches.iter().any(|p| p.id == commit.id())
            }) {
                true
            } else {
                check_commit.is_integrated(commit)?
            }
        };

        let commit_dependencies = commit_dependencies_from_stack(stack_dependencies, commit.id());

        let vcommit = commit_to_vbranch_commit(
            repo,
            stack,
            commit,
            is_integrated,
            true,
            false,
            None,
            None,
            commit_dependencies,
        )?;
        upstream_patches.push(vcommit);
    }
    upstream_patches.reverse();
    // There should be no duplicates, but dedup because the UI cant handle duplicates
    upstream_patches.dedup_by(|a, b| a.id == b.id);

    if !upstream_patches.is_empty() {
        requires_force = true;
    }
    Ok((
        PatchSeries {
            name: stack_branch.name().to_owned(),
            description: stack_branch.description,
            upstream_reference,
            patches,
            upstream_patches,
            pr_number: stack_branch.pr_number,
            archived: stack_branch.archived,
            review_id: stack_branch.review_id,
        },
        requires_force,
    ))
}
