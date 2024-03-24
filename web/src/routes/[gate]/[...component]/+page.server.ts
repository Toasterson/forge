import prisma from '$lib/prisma';
import type { PageServerLoad } from './$types';

export const load = (async ({params, url}) => {

    let gate_id = params.gate;
    let name = params.component;
    let version = url.searchParams.get('version');
    let revision = url.searchParams.get('revision');

    const response = (version !== null && revision !== null) ?
        await prisma.component.findUnique({
        where: {
            name_gateId_version_revision: {
                name: name,
                gateId: gate_id,
                version: version,
                revision: revision,
            },
        }
    }):
        await prisma.component.findMany({
        where: {
            name: name,
            gateId: gate_id,
        },
    });


    return { component: response };
}) satisfies PageServerLoad;