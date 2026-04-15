.PHONY: up down logs build clean test

up:
	docker compose up -d --build

down:
	docker compose down

logs:
	docker compose logs -f

build:
	docker compose build

clean:
	docker compose down --rmi local

test:
	docker compose --profile test up --build --abort-on-container-exit test
