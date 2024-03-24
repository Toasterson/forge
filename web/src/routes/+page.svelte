<script lang="ts">
	import {focusTrap} from "@skeletonlabs/skeleton";

	import type { ActionData } from './$types';

	export let form: ActionData;
</script>

<div class="grid grid-cols-1 m-72">
	<div class="text-2xl my-5">Welcome to the OpenIndiana Forge. Home to countless packages</div>
	<form use:focusTrap={true} method="POST">
		<input name="package_search" class="input" type="text" placeholder="Search ..." />
	</form>
	{#if form }
		<div class="table-container my-10">
			<!-- Native Table Element -->
			<table class="table table-hover">
				<thead>
				<tr>
					<th>Gate</th>
					<th>Name</th>
					<th>Version</th>
					<th>Revision</th>
				</tr>
				</thead>
				<tbody>
				{#each form?.components as { gate, name, version, revison }}
					<tr>
						<td>{gate}</td>
						<td>{name}</td>
						<td>{version}</td>
						<td>{revison}</td>
					</tr>
				{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

