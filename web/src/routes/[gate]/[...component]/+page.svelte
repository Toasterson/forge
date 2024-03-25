<script lang="ts">
    import type { PageData } from './$types';
    import {TabGroup, Tab, TabAnchor, tableMapperValues, type TableSource, Table} from '@skeletonlabs/skeleton';
    import { CodeBlock } from '@skeletonlabs/skeleton';
    export let data: PageData;
    let {name, version, revision, recipe, repology_id, anitya_id} = data.components[0];
    let tabSet: number = 0;

    const versionTable: TableSource = {
        // A list of heading labels.
        head: ['Name', 'Version', 'Revision'],
        // The data visibly shown in your table body UI.
        body: tableMapperValues(data?.components, ['name', 'version', 'revision']),
        // Optional: The data returned when interactive is enabled and a row is clicked.
        meta: tableMapperValues(data?.components, ['gateId', 'name', 'version', 'revision']),
    };

    const dependencyTable: TableSource = {
        // A list of heading labels.
        head: ['Name', 'Kind', 'Build Dependency'],
        // The data visibly shown in your table body UI.
        body: tableMapperValues(recipe?.dependencies, ['name', 'kind', 'dev']),
    };

</script>

<div class="grid grid-cols-1 my-12 mx-48">
    <p class="text-5xl mb-5">Overview for Component: {name}</p>
    <TabGroup justify="justify-around" flex="flex-1">
        <Tab bind:group={tabSet} name="overview" value={0}>
            <span>Overview</span>
        </Tab>
        <Tab bind:group={tabSet} name="dependencies" value={1}>
            <span>Dependencies</span>
        </Tab>
        <Tab bind:group={tabSet} name="tasks" value={2}>
            <span>Tasks</span>
        </Tab>
        <Tab bind:group={tabSet} name="change_sets" value={3}>
            <span>History</span>
        </Tab>
        <svelte:fragment slot="panel">
            {#if tabSet === 0}
                <p class="text-2xl font-bold mb-2">Versions:</p>
                <Table source={versionTable}></Table>
                <p class="text-2xl font-bold mt-5 mb-2">Component Metadata:</p>
                <div class="card p-4 grid grid-cols-5">
                    <div class="p-4">Project</div>
                    <span class="divider-vertical h-20" />
                    <div class="p-4 col-span-3 text-primary-600 underline"><a href={recipe?.project_url}>{recipe?.project_url}</a></div>
                    <div class="p-4">License</div>
                    <span class="divider-vertical h-20" />
                    <div class="p-4 col-span-3">{recipe?.license}</div>
                    <div class="p-4">Summary</div>
                    <span class="divider-vertical h-20" />
                    <div class="p-4 col-span-3">{recipe?.summary}</div>
                    <div class="p-4">Classification</div>
                    <span class="divider-vertical h-20" />
                    <div class="p-4 col-span-3">{recipe?.classification}</div>
                    <div class="p-4">External Resources</div>
                    <span class="divider-vertical h-20" />
                    <div class="p-4 col-span-3 grid grid-cols-1">
                        {#if repology_id}
                            <a class="text-primary-600 underline" rel="external" href={`https://repology.org/tools/project-by?repo=openindiana&name_type=srcname&target_page=project_versions&name=${repology_id}`}>Repology</a>
                        {/if}
                        {#if anitya_id}
                            <a class="text-primary-600 underline" rel="external" href={`https://release-monitoring.org/project/${anitya_id}/`}>Anitya Release monitoring</a>
                        {/if}
                        <a class="text-primary-600 underline" rel="external" href={`https://github.com/OpenIndiana/oi-userland/tree/oi/hipster/components/${name}/Makefile`}>GitHub Sources</a>
                    </div>
                    <div class="p-4">Sources</div>
                    <span class="divider-vertical h-20" />
                    <div class="p-4 col-span-3 grid grid-cols-4">
                        {#each recipe?.sources as {sources}}
                            {#each sources as {Archive, Patch}}
                                {#if Archive}
                                    <div class="py-1.5">Archive:</div>
                                    <div class="py-1.5 col-span-3 text-primary-600 underline"><a href={Archive.src}>{Archive.src}</a></div>
                                {/if}
                                {#if Patch}
                                    <div class="py-1.5">Patch:</div>
                                    <div class="py-1.5 col-span-3">{Patch.bundle_path}</div>
                                {/if}
                            {/each}
                        {/each}
                    </div>
                </div>
                <p class="text-2xl font-bold mt-5 mb-2">Build Instructions:</p>
                {#each recipe?.build_sections as {cmake, meson, script, configure}}
                    {#if configure}
                        <CodeBlock language="bash" code={`./configure ${configure.options.map(({option}) => option).join(' ')}\nmake\nmake install`}></CodeBlock>
                    {/if}
                {/each}
            {:else if tabSet === 1}
                <Table source={dependencyTable}></Table>
            {:else if tabSet === 2}
                <p>To be Created</p>
            {:else if tabSet === 3}
                <p>To be Created</p>
            {/if}
        </svelte:fragment>
    </TabGroup>
</div>