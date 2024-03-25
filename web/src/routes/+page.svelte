<script lang="ts">
	import {focusTrap, tableMapperValues} from "@skeletonlabs/skeleton";
	import { Table } from '@skeletonlabs/skeleton';
	import type { TableSource } from '@skeletonlabs/skeleton';

	import type { ActionData } from './$types';

	export let form: ActionData;

	let tableSimple: TableSource = {
		head: ['Name', 'Version', 'Revision'],
		body: [],
	};
	if (form?.components) {
		tableSimple = {
			// A list of heading labels.
			head: ['Name', 'Version', 'Revision'],
			// The data visibly shown in your table body UI.
			body: tableMapperValues(form?.components, ['name', 'version', 'revision']),
			// Optional: The data returned when interactive is enabled and a row is clicked.
			meta: tableMapperValues(form?.components, ['gateId', 'name', 'version', 'revision']),
		};

	}

	function mySelectionHandler(e: CustomEvent) {
		const [gateId, name, version, revision] = e.detail;
		window.location.replace(`/${gateId}/${name}?version=${version}&revision=${revision}`);
	}
</script>

<div class="grid grid-cols-1 m-72">
	<div class="text-2xl my-5">Welcome to the OpenIndiana Forge. Home to countless packages</div>
	<form use:focusTrap={true} method="POST">
		<input name="package_search" class="input" type="text" placeholder="Search ..." />
	</form>
	{#if form }
		<div class="table-container my-10">
			<Table source={tableSimple} interactive={true} on:selected={mySelectionHandler}></Table>
		</div>
	{/if}
</div>

