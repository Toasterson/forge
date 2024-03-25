FROM nodejs:20

COPY . /app

WORKDIR /app

RUN npm run build

CMD [ "node", "build" ]