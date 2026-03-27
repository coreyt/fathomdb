package cli

import (
	"errors"
	"flag"
	"fmt"
	"io"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/commands"
	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/config"
)

func Main(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprintln(stderr, usage())
		return 2
	}

	cfg := config.Load()

	switch args[0] {
	case "check":
		fs := flag.NewFlagSet("check", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", "", "path to admin bridge binary (optional; enables Layer 2 engine checks)")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		if err := commands.RunCheckWithFeedback(
			*db,
			*bridgeBinary,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "export":
		fs := flag.NewFlagSet("export", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		destination := fs.String("out", "", "path to export destination")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *destination == "" {
			fmt.Fprintln(stderr, "--db and --out are required")
			return 2
		}
		if err := commands.RunExportWithFeedback(
			*db,
			*destination,
			*bridgeBinary,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "trace":
		fs := flag.NewFlagSet("trace", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		sourceRef := fs.String("source-ref", "", "source reference to trace")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *sourceRef == "" {
			fmt.Fprintln(stderr, "--db and --source-ref are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandTraceSource,
			SourceRef:    *sourceRef,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "rebuild":
		fs := flag.NewFlagSet("rebuild", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		target := fs.String("target", "all", "projection target: fts|vec|all")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandRebuildProjections,
			Target:       *target,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "rebuild-missing":
		fs := flag.NewFlagSet("rebuild-missing", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandRebuildMissing,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "regenerate-vectors":
		fs := flag.NewFlagSet("regenerate-vectors", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		configPath := fs.String("config", "", "path to TOML or JSON vector regeneration contract")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *configPath == "" {
			fmt.Fprintln(stderr, "--db and --config are required")
			return 2
		}
		if err := commands.RunRegenerateVectorsWithFeedback(
			*db,
			*bridgeBinary,
			*configPath,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "excise":
		fs := flag.NewFlagSet("excise", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		sourceRef := fs.String("source-ref", "", "source reference to excise")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *sourceRef == "" {
			fmt.Fprintln(stderr, "--db and --source-ref are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandExciseSource,
			SourceRef:    *sourceRef,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "recover":
		fs := flag.NewFlagSet("recover", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to the (possibly corrupt) source sqlite database")
		dest := fs.String("dest", "", "path where the recovered database will be written (must not already exist)")
		bridgeBinary := fs.String("bridge", "", "path to admin bridge binary (optional; bootstraps fathomdb schema and enables Layer 2 checks)")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *dest == "" {
			fmt.Fprintln(stderr, "--db and --dest are required")
			return 2
		}
		if err := commands.RunRecoverWithFeedback(
			*db,
			*dest,
			*bridgeBinary,
			"",
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "repair":
		fs := flag.NewFlagSet("repair", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary (optional; used for follow-up checks only)")
		target := fs.String("target", commands.RepairTargetAll, "repair target: all|duplicate-active|runtime-fk|orphaned-chunks")
		dryRun := fs.Bool("dry-run", false, "report planned repairs without mutating the database")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		if err := commands.RunRepairWithFeedback(
			*db,
			*bridgeBinary,
			"",
			*target,
			*dryRun,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "version":
		if err := commands.RunVersion(stdout); err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
		return 0
	default:
		fmt.Fprintln(stderr, usage())
		return 2
	}
}

func commandExitCode(err error) int {
	var coded interface{ ExitCode() int }
	if errors.As(err, &coded) {
		return coded.ExitCode()
	}
	return bridge.ExitCodeFromError(err)
}

func usage() string {
	return "usage: fathom-integrity <check|export|trace|rebuild|rebuild-missing|regenerate-vectors|excise|recover|repair|version> [flags]"
}
