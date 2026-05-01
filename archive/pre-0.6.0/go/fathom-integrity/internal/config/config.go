package config

import "os"

const defaultBridgeBinary = "fathomdb-admin-bridge"

type Config struct {
	DatabasePath string
	BridgeBinary string
}

func Load() Config {
	return Config{
		DatabasePath: os.Getenv("FATHOM_DB_PATH"),
		BridgeBinary: defaultString(os.Getenv("FATHOM_ADMIN_BRIDGE"), defaultBridgeBinary),
	}
}

func defaultString(value, fallback string) string {
	if value == "" {
		return fallback
	}
	return value
}
