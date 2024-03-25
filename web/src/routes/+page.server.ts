import type { Actions } from './$types';
import prisma from '$lib/prisma';

export const actions = {
    default: async ({request}) => {
        const data = await request.formData();
        const component_search = data.get('package_search')
        const components = await prisma.component.findMany({
            select: {
                name: true,
                gateId: true,
                version: true,
                revision: true,
            },
            where:{
                name: {
                    contains: component_search?.toString()
                }
            },
            orderBy: {
                version: "asc"
            }
        });
        return {
            components
        }
    },
} satisfies Actions;