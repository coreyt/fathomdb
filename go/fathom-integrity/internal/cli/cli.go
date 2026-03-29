package cli

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"strings"

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
	case "restore-logical-id":
		fs := flag.NewFlagSet("restore-logical-id", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		logicalID := fs.String("logical-id", "", "logical id to restore")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *logicalID == "" {
			fmt.Fprintln(stderr, "--db and --logical-id are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandRestoreLogicalID,
			LogicalID:    *logicalID,
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
	case "purge-logical-id":
		fs := flag.NewFlagSet("purge-logical-id", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		logicalID := fs.String("logical-id", "", "retired logical id to purge")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *logicalID == "" {
			fmt.Fprintln(stderr, "--db and --logical-id are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandPurgeLogicalID,
			LogicalID:    *logicalID,
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
	case "trace-operational":
		fs := flag.NewFlagSet("trace-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to trace")
		recordKey := fs.String("record-key", "", "optional record key to narrow the trace")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" {
			fmt.Fprintln(stderr, "--db and --collection are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandTraceOperationalCollection,
			CollectionName: *collectionName,
			RecordKey:      *recordKey,
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
	case "disable-operational":
		fs := flag.NewFlagSet("disable-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to disable")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" {
			fmt.Fprintln(stderr, "--db and --collection are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandDisableOperationalCollection,
			CollectionName: *collectionName,
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
	case "compact-operational":
		fs := flag.NewFlagSet("compact-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to compact")
		dryRun := fs.Bool("dry-run", false, "report the compaction result without mutating the database")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" {
			fmt.Fprintln(stderr, "--db and --collection are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandCompactOperationalCollection,
			CollectionName: *collectionName,
			DryRun:         *dryRun,
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
	case "purge-operational":
		fs := flag.NewFlagSet("purge-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to purge")
		beforeTimestamp := fs.Int64("before", 0, "delete mutations older than this unix timestamp")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" || *beforeTimestamp == 0 {
			fmt.Fprintln(stderr, "--db, --collection, and --before are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:    *db,
			Command:         bridge.CommandPurgeOperationalCollection,
			CollectionName:  *collectionName,
			BeforeTimestamp: *beforeTimestamp,
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
	case "rebuild-operational-current":
		fs := flag.NewFlagSet("rebuild-operational-current", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "optional operational collection name to rebuild")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandRebuildOperationalCurrent,
			CollectionName: *collectionName,
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
		defaultPolicy := bridge.DefaultVectorGeneratorPolicy()
		generatorTimeoutMS := fs.Uint64("generator-timeout-ms", defaultPolicy.TimeoutMS, "wall-clock timeout for the external vector generator")
		generatorMaxStdoutBytes := fs.Int("generator-max-stdout-bytes", defaultPolicy.MaxStdoutBytes, "maximum stdout bytes allowed from the external vector generator")
		generatorMaxStderrBytes := fs.Int("generator-max-stderr-bytes", defaultPolicy.MaxStderrBytes, "maximum stderr bytes allowed from the external vector generator")
		generatorMaxInputBytes := fs.Int("generator-max-input-bytes", defaultPolicy.MaxInputBytes, "maximum JSON stdin bytes sent to the external vector generator")
		generatorMaxChunks := fs.Int("generator-max-chunks", defaultPolicy.MaxChunks, "maximum chunk count allowed in one vector regeneration run")
		var generatorAllowedRoots stringSliceFlag
		var generatorPreserveEnv stringSliceFlag
		fs.Var(&generatorAllowedRoots, "generator-allowed-root", "allowlisted root for the external vector generator executable (repeatable)")
		fs.Var(&generatorPreserveEnv, "generator-preserve-env", "environment variable to preserve for the external vector generator (repeatable)")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *configPath == "" {
			fmt.Fprintln(stderr, "--db and --config are required")
			return 2
		}
		if *generatorMaxStdoutBytes <= 0 || *generatorMaxStderrBytes <= 0 || *generatorMaxInputBytes <= 0 || *generatorMaxChunks <= 0 {
			fmt.Fprintln(stderr, "generator limits must be greater than zero")
			return 2
		}
		if err := commands.RunRegenerateVectorsWithFeedback(
			*db,
			*bridgeBinary,
			*configPath,
			&bridge.VectorGeneratorPolicy{
				TimeoutMS:                     *generatorTimeoutMS,
				MaxStdoutBytes:                *generatorMaxStdoutBytes,
				MaxStderrBytes:                *generatorMaxStderrBytes,
				MaxInputBytes:                 *generatorMaxInputBytes,
				MaxChunks:                     *generatorMaxChunks,
				RequireAbsoluteExecutable:     defaultPolicy.RequireAbsoluteExecutable,
				RejectWorldWritableExecutable: defaultPolicy.RejectWorldWritableExecutable,
				AllowedExecutableRoots:        append([]string(nil), generatorAllowedRoots...),
				PreserveEnvVars:               append([]string(nil), generatorPreserveEnv...),
			},
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

type stringSliceFlag []string

func (s *stringSliceFlag) String() string {
	return strings.Join(*s, ",")
}

func (s *stringSliceFlag) Set(value string) error {
	*s = append(*s, value)
	return nil
}

func commandExitCode(err error) int {
	var coded interface{ ExitCode() int }
	if errors.As(err, &coded) {
		return coded.ExitCode()
	}
	return bridge.ExitCodeFromError(err)
}

func usage() string {
	return "usage: fathom-integrity <check|export|trace|restore-logical-id|purge-logical-id|trace-operational|disable-operational|compact-operational|purge-operational|rebuild|rebuild-operational-current|rebuild-missing|regenerate-vectors|excise|recover|repair|version> [flags]"
}
