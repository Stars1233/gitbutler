<script lang="ts">
	import { goto } from '$app/navigation';
	import AnalyticsMonitor from '$components/AnalyticsMonitor.svelte';
	import Chrome from '$components/Chrome.svelte';
	import FileMenuAction from '$components/FileMenuAction.svelte';
	import FullviewLoading from '$components/FullviewLoading.svelte';
	import History from '$components/History.svelte';
	import IrcPopups from '$components/IrcPopups.svelte';
	import MetricsReporter from '$components/MetricsReporter.svelte';
	import NoBaseBranch from '$components/NoBaseBranch.svelte';
	import NotOnGitButlerBranch from '$components/NotOnGitButlerBranch.svelte';
	import ProblemLoadingRepo from '$components/ProblemLoadingRepo.svelte';
	import ProjectSettingsMenuAction from '$components/ProjectSettingsMenuAction.svelte';
	import ReduxResult from '$components/ReduxResult.svelte';
	import { BaseBranch } from '$lib/baseBranch/baseBranch';
	import BaseBranchService from '$lib/baseBranch/baseBranchService.svelte';
	import { BranchService } from '$lib/branches/branchService.svelte';
	import { GitBranchService } from '$lib/branches/gitBranch';
	import { SettingsService } from '$lib/config/appSettingsV2';
	import { showHistoryView } from '$lib/config/config';
	import { StackingReorderDropzoneManagerFactory } from '$lib/dragging/stackingReorderDropzoneManager';
	import { Feed } from '$lib/feed/feed';
	import { FocusManager } from '$lib/focus/focusManager.svelte';
	import { DefaultForgeFactory } from '$lib/forge/forgeFactory.svelte';
	import { GitHubClient } from '$lib/forge/github/githubClient';
	import { GitLabClient } from '$lib/forge/gitlab/gitlabClient.svelte';
	import { GitLabState } from '$lib/forge/gitlab/gitlabState.svelte';
	import { TemplateService } from '$lib/forge/templateService';
	import { HistoryService } from '$lib/history/history';
	import { ModeService } from '$lib/mode/modeService';
	import { showError, showInfo } from '$lib/notifications/toasts';
	import { ProjectsService } from '$lib/project/projectsService';
	import { getSecretsService } from '$lib/secrets/secretsService';
	import { IdSelection } from '$lib/selection/idSelection.svelte';
	import { UncommittedService } from '$lib/selection/uncommittedService.svelte';
	import { StackService } from '$lib/stacks/stackService.svelte';
	import { ClientState } from '$lib/state/clientState.svelte';
	import { debounce } from '$lib/utils/debounce';
	import { WorktreeService } from '$lib/worktree/worktreeService.svelte';
	import { getContext } from '@gitbutler/shared/context';
	import { onDestroy, setContext, untrack, type Snippet } from 'svelte';
	import type { ProjectMetrics } from '$lib/metrics/projectMetrics';
	import type { LayoutData } from './$types';

	const { data, children: pageChildren }: { data: LayoutData; children: Snippet } = $props();

	const { projectId, userService, fetchSignal, posthog, projectMetrics, tauri } = $derived(data);

	const baseBranchService = getContext(BaseBranchService);
	const repoInfoResponse = $derived(baseBranchService.repo(projectId));
	const repoInfo = $derived(repoInfoResponse.current.data);
	const baseBranchResponse = $derived(baseBranchService.baseBranch(projectId));
	const baseBranch = $derived(baseBranchResponse.current.data);
	const pushRepoResponse = $derived(baseBranchService.pushRepo(projectId));
	const forkInfo = $derived(pushRepoResponse.current.data);
	const baseBranchName = $derived(baseBranch?.shortName);
	const branchService = getContext(BranchService);

	const stackService = getContext(StackService);
	const feed = $derived(new Feed(tauri, projectId, stackService));
	const modeService = $derived(new ModeService(projectId, stackService));
	$effect.pre(() => {
		setContext(ModeService, modeService);
	});

	const secretService = getSecretsService();
	const gitLabState = $derived(new GitLabState(secretService, repoInfo, projectId));
	$effect.pre(() => {
		setContext(GitLabState, gitLabState);
	});
	const gitLabClient = getContext(GitLabClient);
	$effect.pre(() => {
		gitLabClient.set(gitLabState);
	});

	const user = $derived(userService.user);
	const accessToken = $derived($user?.github_access_token);

	const gitHubClient = getContext(GitHubClient);
	$effect.pre(() => gitHubClient.setToken(accessToken));
	$effect.pre(() => gitHubClient.setRepo({ owner: repoInfo?.owner, repo: repoInfo?.name }));

	const projectsService = getContext(ProjectsService);
	const projectsResult = $derived(projectsService.projects());
	const projects = $derived(projectsResult.current.data);

	$effect.pre(() => {
		const stackingReorderDropzoneManagerFactory = new StackingReorderDropzoneManagerFactory(
			projectId,
			stackService
		);

		setContext(StackingReorderDropzoneManagerFactory, stackingReorderDropzoneManagerFactory);
	});

	$effect.pre(() => {
		setContext(HistoryService, data.historyService);
		setContext(TemplateService, data.templateService);
		setContext(BaseBranch, baseBranch);
		setContext(GitBranchService, data.gitBranchService);

		// Cloud related services
		setContext(Feed, feed);
	});

	const focusManager = new FocusManager();
	setContext(FocusManager, focusManager);

	let intervalId: any;

	const forgeFactory = getContext(DefaultForgeFactory);

	// Refresh base branch if git fetch event is detected.
	const mode = $derived(modeService.mode);
	const head = $derived(modeService.head);

	// TODO: can we eliminate the need to debounce?
	const fetch = $derived(fetchSignal.event);
	const debouncedBaseBranchRefresh = debounce(async () => {
		await baseBranchService.refreshBaseBranch(projectId);
	}, 500);
	$effect(() => {
		if ($fetch || $head) debouncedBaseBranchRefresh();
	});

	// TODO: can we eliminate the need to debounce?
	const debouncedRemoteBranchRefresh = debounce(
		async () => await branchService.refresh(projectId),
		500
	);

	$effect(() => {
		if (baseBranch || $head || $fetch) debouncedRemoteBranchRefresh();
	});

	const gitlabConfigured = $derived(gitLabState.configured);

	$effect(() => {
		forgeFactory.setConfig({
			repo: repoInfo,
			pushRepo: forkInfo,
			baseBranch: baseBranchName,
			githubAuthenticated: !!$user?.github_access_token,
			gitlabAuthenticated: !!$gitlabConfigured,
			forgeOverride: projects?.find((project) => project.id === projectId)?.forge_override
		});
	});

	$effect(() => {
		posthog.setPostHogRepo(repoInfo);
		return () => {
			posthog.setPostHogRepo(undefined);
		};
	});

	// Once on load and every time the project id changes
	$effect(() => {
		if (projectId) {
			setupFetchInterval();
		} else {
			goto('/onboarding');
		}
	});

	// TODO(mattias): This is an ugly hack, fix it somehow?
	// I want to flush project metrics to local storage before e.g. switching
	// to a different project. Since `projectMetrics` is defined in layout.ts
	// we get no heads up when it is about to change, and reactively updated
	// in this scope through `LayoutData`. Even at time of unMount in e.g.
	// metrics reporter it seems as if the projectMetrics variable is already
	// referencing the new instance.
	let lastProjectMetrics: ProjectMetrics | undefined;
	$effect(() => {
		if (lastProjectMetrics) {
			lastProjectMetrics.saveToLocalStorage();
		}
		lastProjectMetrics = projectMetrics;
		projectMetrics.loadFromLocalStorage();
	});

	async function fetchRemoteForProject() {
		await baseBranchService.fetchFromRemotes(projectId, 'auto');
	}

	function setupFetchInterval() {
		const autoFetchIntervalMinutes = $settingsStore?.fetch.autoFetchIntervalMinutes || 15;
		if (autoFetchIntervalMinutes < 0) {
			return;
		}
		fetchRemoteForProject();
		clearFetchInterval();
		const intervalMs = autoFetchIntervalMinutes * 60 * 1000; // 15 minutes
		intervalId = setInterval(async () => {
			await fetchRemoteForProject();
		}, intervalMs);
	}

	function clearFetchInterval() {
		if (intervalId) clearInterval(intervalId);
	}

	const settingsService = getContext(SettingsService);
	const settingsStore = settingsService.appSettings;

	onDestroy(() => {
		clearFetchInterval();
	});

	async function setActiveProjectOrRedirect() {
		// Optimistically assume the project is viewable
		try {
			const info = await projectsService.setActiveProject(projectId);
			if (!info.is_exclusive) {
				showInfo(
					'Just FYI, this project is already open in another window',
					'There might be some unexpected behavior if you open it in multiple windows'
				);
			}
			if (info.db_error) {
				showError('The database was corrupted', info.db_error);
			}
			if (info.headsup) {
				showError('Important PSA', info.headsup);
			}
		} catch (error: unknown) {
			showError('Failed to set the project active', error);
		}
	}

	$effect(() => {
		setActiveProjectOrRedirect();
	});

	// Clear the backend API when the project id changes.
	const clientState = getContext(ClientState);
	$effect(() => {
		if (projectId) {
			clientState.backendApi.util.resetApiState();
		}
	});

	// TODO: Can we combine WorktreeService and UncommittedService? The former
	// is powered by RTKQ, while the latter is a custom slice, but perhaps
	// they could be still be contained within the same .svelte.ts file.
	const worktreeService = getContext(WorktreeService);
	const uncommittedService = getContext(UncommittedService);
	const worktreeDataResult = $derived(worktreeService.worktreeData(projectId));
	const worktreeData = $derived(worktreeDataResult.current.data);

	// This effect is a sort of bridge between rtkq and the custom slice.
	$effect(() => {
		if (worktreeData) {
			untrack(() => {
				uncommittedService.updateData({
					changes: worktreeData.rawChanges,
					// If assignments are not enabled we override the stack id to prevent
					// files from becoming hidden when toggling on/off.
					assignments: worktreeData.hunkAssignments
				});
			});
		}
	});

	// Here we clear any expired file selections. Note that the notion of
	// file selection is related to selecting things with checkmarks, and
	// that this `IdSelection` class should be deprecated in favor of
	// combining it with `UncommittedService`.
	const idSelection = getContext(IdSelection);
	const affectedPaths = $derived(worktreeData?.rawChanges.map((c) => c.path));
	$effect(() => {
		if (affectedPaths) {
			untrack(() => {
				idSelection.retain(affectedPaths);
			});
		}
	});

	// Listen for stack details updates from the backend.
	$effect(() => {
		stackService.stackDetailsUpdateListener(projectId);
	});
</script>

<ProjectSettingsMenuAction {projectId} />
<FileMenuAction />

<ReduxResult {projectId} result={baseBranchResponse.current}>
	{#snippet children(baseBranch, { projectId })}
		{#if !baseBranch}
			<NoBaseBranch {projectId} />
		{:else if baseBranch}
			{#if $mode?.type === 'OpenWorkspace' || $mode?.type === 'Edit'}
				<div class="view-wrap" role="group" ondragover={(e) => e.preventDefault()}>
					<Chrome {projectId} sidebarDisabled={$mode?.type === 'Edit'}>
						{@render pageChildren()}
					</Chrome>
					{#if $showHistoryView}
						<History {projectId} onHide={() => ($showHistoryView = false)} />
					{/if}
				</div>
			{:else if $mode?.type === 'OutsideWorkspace'}
				<NotOnGitButlerBranch {projectId} {baseBranch} />
			{/if}
		{/if}
	{/snippet}
	{#snippet loading()}
		<FullviewLoading />
	{/snippet}
	{#snippet error(baseError)}
		<ProblemLoadingRepo {projectId} error={baseError} />
	{/snippet}
</ReduxResult>

<!-- {#if $settingsStore?.featureFlags.v3} -->
<IrcPopups />
<!-- {/if} -->

<!-- Mounting metrics reporter in the board ensures dependent services are subscribed to. -->
<MetricsReporter {projectId} {projectMetrics} />
<AnalyticsMonitor {projectId} />

<style>
	.view-wrap {
		display: flex;
		position: relative;
		width: 100%;
	}
</style>
