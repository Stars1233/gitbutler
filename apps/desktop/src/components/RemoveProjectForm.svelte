<script lang="ts">
	import { goto } from '$app/navigation';
	import ReduxResult from '$components/ReduxResult.svelte';
	import RemoveProjectButton from '$components/RemoveProjectButton.svelte';
	import { showError } from '$lib/notifications/toasts';
	import { ProjectsService } from '$lib/project/projectsService';
	import { getContext } from '@gitbutler/shared/context';
	import SectionCard from '@gitbutler/ui/SectionCard.svelte';
	import * as toasts from '@gitbutler/ui/toasts';

	const { projectId }: { projectId: string } = $props();

	const projectsService = getContext(ProjectsService);
	const projectResult = $derived(projectsService.getProject(projectId));

	let isDeleting = $state(false);

	async function onDeleteClicked() {
		isDeleting = true;
		try {
			await projectsService.deleteProject(projectId);
			goto('/');
			toasts.success('Project deleted');
		} catch (err: any) {
			console.error(err);
			showError('Failed to delete project', err);
		} finally {
			isDeleting = false;
		}
	}
</script>

<ReduxResult {projectId} result={projectResult.current}>
	{#snippet children(project)}
		<SectionCard>
			{#snippet title()}
				Remove project
			{/snippet}
			{#snippet caption()}
				Removing projects from GitButler only clears configuration — your code stays safe.
			{/snippet}
			<div>
				<RemoveProjectButton projectTitle={project.title} {isDeleting} {onDeleteClicked} />
			</div>
		</SectionCard>
	{/snippet}
</ReduxResult>
