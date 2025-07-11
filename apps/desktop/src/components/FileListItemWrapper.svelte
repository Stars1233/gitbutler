<script lang="ts">
	import FileContextMenu from '$components/FileContextMenu.svelte';
	import { BranchStack } from '$lib/branches/branch';
	import { SelectedOwnership } from '$lib/branches/ownership';
	import { getLocalCommits, getLocalAndRemoteCommits } from '$lib/commits/contexts';
	import { getCommitStore } from '$lib/commits/contexts';
	import { draggableChips, type DraggableConfig } from '$lib/dragging/draggable';
	import { FileDropData } from '$lib/dragging/draggables';
	import { DropzoneRegistry } from '$lib/dragging/registry';
	import { LocalFile } from '$lib/files/file';
	import { type AnyFile } from '$lib/files/file';
	import { getLockText } from '$lib/files/lock';
	import { FileIdSelection } from '$lib/selection/fileIdSelection';
	import { itemsSatisfy } from '$lib/utils/array';
	import { computeFileStatus } from '$lib/utils/fileStatus';
	import { getContext, maybeGetContextStore } from '@gitbutler/shared/context';
	import FileListItem from '@gitbutler/ui/file/FileListItem.svelte';
	import type { Writable } from 'svelte/store';

	interface Props {
		file: AnyFile;
		isUnapplied: boolean;
		selected: boolean;
		showCheckbox: boolean;
		readonly: boolean;
		onclick: (e: MouseEvent) => void;
		onkeydown?: (e: KeyboardEvent) => void;
		projectId?: string;
	}

	const {
		file,
		isUnapplied,
		selected,
		showCheckbox,
		readonly,
		onclick,
		onkeydown,
		projectId
	}: Props = $props();

	const stack = maybeGetContextStore(BranchStack);
	const branchId = $derived($stack?.id);
	const selectedOwnership: Writable<SelectedOwnership> | undefined =
		maybeGetContextStore(SelectedOwnership);
	const fileIdSelection = getContext(FileIdSelection);
	const dropzoneRegistry = getContext(DropzoneRegistry);
	const commit = getCommitStore();

	// TODO: Refactor this into something more meaningful.
	const localCommits = file instanceof LocalFile ? getLocalCommits() : undefined;
	const remoteCommits = file instanceof LocalFile ? getLocalAndRemoteCommits() : undefined;
	let lockedIds = file.lockedIds;
	const lockText = $derived(
		lockedIds.length > 0 && $localCommits
			? getLockText(lockedIds, ($localCommits || []).concat($remoteCommits || []))
			: ''
	);
	const selectedFiles = fileIdSelection.files;
	const draggable = !readonly && !isUnapplied;

	let contextMenu = $state<ReturnType<typeof FileContextMenu>>();
	let draggableEl: HTMLDivElement | undefined = $state();
	let indeterminate = $state(false);
	let checked = $state(false);

	$effect(() => {
		if (file && $selectedOwnership) {
			const hunksContained = itemsSatisfy(file.hunks, (h) =>
				$selectedOwnership?.isSelected(file.id, h.id)
			);
			checked = hunksContained === 'all';
			indeterminate = hunksContained === 'some';
		}
	});

	// TODO: Refactor to use this as a Svelte action, e.g. `use:draggableChips()`.
	let chips:
		| {
				update: (opts: DraggableConfig) => void;
				destroy: () => void;
		  }
		| undefined;

	// Manage the lifecycle of the draggable chips.
	$effect(() => {
		if (draggableEl) {
			const dropData = new FileDropData(branchId || '', file, $commit, selectedFiles);
			const config: DraggableConfig = {
				label: `${file.filename}`,
				filePath: file.path,
				data: dropData,
				disabled: !draggable,
				viewportId: 'board-viewport',
				selector: '.selected-draggable',
				chipType: 'file',
				dropzoneRegistry
			};
			if (chips) {
				chips.update(config);
			} else {
				chips = draggableChips(draggableEl, config);
			}
		} else {
			chips?.destroy();
		}

		return () => {
			chips?.destroy();
		};
	});
</script>

{#if projectId}
	<FileContextMenu
		bind:this={contextMenu}
		trigger={draggableEl}
		{isUnapplied}
		stackId={$stack?.id}
		{projectId}
		isBinary={file.binary}
	/>
{/if}

<FileListItem
	id={file.id}
	bind:ref={draggableEl}
	filePath={file.path}
	fileStatus={computeFileStatus(file)}
	{selected}
	{showCheckbox}
	{checked}
	{indeterminate}
	{draggable}
	{onclick}
	{onkeydown}
	locked={file.locked}
	conflicted={file.conflicted}
	{lockText}
	oncheck={(e) => {
		const isChecked = e.currentTarget.checked;
		selectedOwnership?.update((ownership) => {
			if (isChecked) {
				file.hunks.forEach((h) => ownership.select(file.id, h));
			} else {
				file.hunks.forEach((h) => ownership.ignore(file.id, h.id));
			}
			return ownership;
		});

		if ($selectedFiles.length > 0 && $selectedFiles.includes(file)) {
			if (isChecked) {
				$selectedFiles.forEach((f) => {
					selectedOwnership?.update((ownership) => {
						f.hunks.forEach((h) => ownership.select(f.id, h));
						return ownership;
					});
				});
			} else {
				$selectedFiles.forEach((f) => {
					selectedOwnership?.update((ownership) => {
						f.hunks.forEach((h) => ownership.ignore(f.id, h.id));
						return ownership;
					});
				});
			}
		}
	}}
	oncontextmenu={(e) => {
		if (fileIdSelection.has(file.id, $commit?.id)) {
			contextMenu?.open(e, { files: $selectedFiles });
		} else {
			contextMenu?.open(e, { files: [file] });
		}
	}}
/>
