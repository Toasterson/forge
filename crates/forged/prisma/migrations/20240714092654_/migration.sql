-- AlterTable
ALTER TABLE "ComponentChange" ADD COLUMN     "package_meta" JSONB NOT NULL DEFAULT '{}';
