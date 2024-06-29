/*
  Warnings:

  - Changed the type of `patches` on the `Component` table. No cast exists, the column would be dropped and recreated, which cannot be done if there is data, since the column is required.

*/
-- AlterTable
ALTER TABLE "Component" DROP COLUMN "patches",
ADD COLUMN     "patches" JSONB NOT NULL;
