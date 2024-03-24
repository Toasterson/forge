<script>
	import {focusTrap} from "@skeletonlabs/skeleton";
</script>

<div class="grid grid-cols-1 m-72">
	<div class="text-2xl my-5">Welcome to the OpenIndiana Forge. Home to countless packages</div>
	<form use:focusTrap={true}>
		<input class="input" type="text" placeholder="Search ..." />
	</form>
</div>