-- CreateEnum
CREATE TYPE "ChangeRequestState" AS ENUM ('Open', 'Draft', 'Closed', 'Applied');

-- AlterTable
ALTER TABLE "ChangeRequest" ADD COLUMN     "state" "ChangeRequestState" NOT NULL DEFAULT 'Closed';
