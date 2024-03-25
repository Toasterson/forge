import prisma from '$lib/prisma';
import type {PageServerLoad} from './$types';

export const load = (async ({params, url}) => {

    let gate_id = params.gate;
    let name = params.component;
    let version = url.searchParams.get('version');
    let revision = url.searchParams.get('revision');

    const components = await prisma.component.findMany({
        where: {
            name: name,
            gateId: gate_id,
        },
        orderBy: {
            version: "desc",
        }
    });

    const gate = await prisma.gate.findUnique({
        where: {
            id: gate_id,
        }
    });

    return {
        name,
        gate,
        components,
        version,
        revision,
    };
}) satisfies PageServerLoad;