-- CreateExtension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- CreateEnum
CREATE TYPE "KeyType" AS ENUM ('Ed25519');

-- CreateEnum
CREATE TYPE "ComponentChangeKind" AS ENUM ('Added', 'Updated', 'Removed');

-- CreateTable
CREATE TABLE "Domain" (
    "id" UUID NOT NULL,
    "dnsName" TEXT NOT NULL,

    CONSTRAINT "Domain_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Actor" (
    "id" UUID NOT NULL,
    "displayName" TEXT NOT NULL,
    "handle" TEXT NOT NULL,
    "domainId" UUID NOT NULL,

    CONSTRAINT "Actor_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Key" (
    "id" UUID NOT NULL,
    "actorId" UUID NOT NULL,
    "name" TEXT NOT NULL,
    "private_key" TEXT NOT NULL,
    "public_key" TEXT NOT NULL,
    "key_type" "KeyType" NOT NULL DEFAULT 'Ed25519',

    CONSTRAINT "Key_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Publisher" (
    "id" UUID NOT NULL,
    "name" TEXT NOT NULL,

    CONSTRAINT "Publisher_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "PackageRepository" (
    "id" UUID NOT NULL,
    "name" TEXT NOT NULL,
    "publisherId" UUID NOT NULL,

    CONSTRAINT "PackageRepository_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Gate" (
    "id" UUID NOT NULL,
    "name" TEXT NOT NULL,
    "version" TEXT NOT NULL,
    "branch" TEXT NOT NULL,
    "transforms" JSONB NOT NULL,
    "publisherId" UUID NOT NULL,

    CONSTRAINT "Gate_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Component" (
    "name" TEXT NOT NULL,
    "version" TEXT NOT NULL,
    "revision" TEXT NOT NULL,
    "anitya_id" TEXT,
    "repology_id" TEXT,
    "project_url" TEXT NOT NULL,
    "recipe" JSONB NOT NULL,
    "patches" TEXT[],
    "scripts" TEXT[],
    "archives" TEXT[],
    "packages" JSONB NOT NULL,
    "gateId" UUID NOT NULL,

    CONSTRAINT "Component_pkey" PRIMARY KEY ("name","gateId","version","revision")
);

-- CreateTable
CREATE TABLE "ComponentChange" (
    "id" UUID NOT NULL,
    "kind" "ComponentChangeKind" NOT NULL,
    "diff" JSONB NOT NULL,
    "recipe" JSONB NOT NULL,
    "version" TEXT NOT NULL,
    "revision" TEXT NOT NULL,
    "patches" TEXT[],
    "scripts" TEXT[],
    "archives" TEXT[],
    "componentName" TEXT,
    "componentVersion" TEXT,
    "componentRevision" TEXT,
    "gateId" UUID,
    "changeRequestId" UUID NOT NULL,

    CONSTRAINT "ComponentChange_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "ChangeRequest" (
    "id" UUID NOT NULL,
    "processing" BOOLEAN NOT NULL DEFAULT false,
    "waitForRequestId" UUID,
    "build_order" TEXT[],
    "external_reference" TEXT,

    CONSTRAINT "ChangeRequest_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "BuildJob" (
    "id" UUID NOT NULL,
    "componentName" TEXT,
    "componentVersion" TEXT,
    "componentRevision" TEXT,
    "gateId" UUID,
    "changeRequestId" UUID NOT NULL,

    CONSTRAINT "BuildJob_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "Domain_dnsName_key" ON "Domain"("dnsName");

-- CreateIndex
CREATE UNIQUE INDEX "Actor_handle_key" ON "Actor"("handle");

-- CreateIndex
CREATE UNIQUE INDEX "Publisher_name_key" ON "Publisher"("name");

-- AddForeignKey
ALTER TABLE "Actor" ADD CONSTRAINT "Actor_domainId_fkey" FOREIGN KEY ("domainId") REFERENCES "Domain"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Key" ADD CONSTRAINT "Key_actorId_fkey" FOREIGN KEY ("actorId") REFERENCES "Actor"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "PackageRepository" ADD CONSTRAINT "PackageRepository_publisherId_fkey" FOREIGN KEY ("publisherId") REFERENCES "Publisher"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Gate" ADD CONSTRAINT "Gate_publisherId_fkey" FOREIGN KEY ("publisherId") REFERENCES "Publisher"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Component" ADD CONSTRAINT "Component_gateId_fkey" FOREIGN KEY ("gateId") REFERENCES "Gate"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "ComponentChange" ADD CONSTRAINT "ComponentChange_componentName_gateId_componentVersion_comp_fkey" FOREIGN KEY ("componentName", "gateId", "componentVersion", "componentRevision") REFERENCES "Component"("name", "gateId", "version", "revision") ON DELETE SET NULL ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "ComponentChange" ADD CONSTRAINT "ComponentChange_gateId_fkey" FOREIGN KEY ("gateId") REFERENCES "Gate"("id") ON DELETE SET NULL ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "ComponentChange" ADD CONSTRAINT "ComponentChange_changeRequestId_fkey" FOREIGN KEY ("changeRequestId") REFERENCES "ChangeRequest"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "ChangeRequest" ADD CONSTRAINT "ChangeRequest_waitForRequestId_fkey" FOREIGN KEY ("waitForRequestId") REFERENCES "ChangeRequest"("id") ON DELETE SET NULL ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "BuildJob" ADD CONSTRAINT "BuildJob_componentName_gateId_componentVersion_componentRe_fkey" FOREIGN KEY ("componentName", "gateId", "componentVersion", "componentRevision") REFERENCES "Component"("name", "gateId", "version", "revision") ON DELETE SET NULL ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "BuildJob" ADD CONSTRAINT "BuildJob_changeRequestId_fkey" FOREIGN KEY ("changeRequestId") REFERENCES "ChangeRequest"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
